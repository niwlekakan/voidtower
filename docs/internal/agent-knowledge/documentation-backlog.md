
## C3-02 retrospective — 2026-09-13

- End-user/operator documentation added: `docs/agent/inventory-upload.md`, covering node binding, canonical host prerequisite, replay/conflict behavior, reconciliation safety, and runtime limitations.
- Future documentation required: C3-03 upload scheduling/backoff, service installation, outage/restart recovery, upgrade, rollback, and the supported node enrollment-to-host-adoption workflow.
- Reusable checks: `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features`; `cd backend && cargo test api::cmdb::tests --all-features`; `cd backend && cargo test --all-targets --all-features`.


## C3-03 retrospective — 2026-09-14

- End-user/operator documentation added: `docs/agent/linux-agent-service.md`, covering the foreground service unit, protected state, installation, upgrade, rollback, outage behavior, and qualification boundary.
- Future documentation required: a supported-platform runtime runbook with observed systemd status, controller outage/restart recovery, binary upgrade, rollback, and enrollment-to-host-adoption evidence.
- Reusable checks: `cd backend && cargo test collector --all-features`; `cd backend && cargo test agent::transport --all-features`; `cd backend && cargo test agent::supervision --all-features`; `cd backend && cargo test --all-targets --all-features`.

## C3-03 bounded command I/O — 2026-09-15

- End-user documentation updated: `docs/agent/linux-agent-service.md` now states that collector stdout/stderr are concurrently drained within bounded limits and overflow skips the snapshot.
- Future documentation required: the supported-platform runbook still needs observed systemd status, protected state, real collection/upload, controller outage/restart recovery, upgrade, rollback, and enrollment-to-host-adoption evidence.
- Reusable check: `cd backend && cargo test collector::tests --all-features`.


## C3-03 runtime qualification checkpoint — 2026-09-14

- Required future end-user/operator documentation remains a supported-platform runtime runbook with observed systemd status, protected state, real collection/upload, controller outage/restart recovery, binary upgrade, rollback, and enrollment-to-host-adoption evidence.
- This checkpoint changed no end-user documentation because the required host runtime is unavailable; existing `docs/agent/linux-agent-service.md` accurately labels qualification as blocked.
- Reusable checks: `cd backend && cargo test collector --all-features`; `cd backend && cargo test agent::transport --all-features`; `cd backend && cargo test agent::supervision --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`.


## C3-03 runtime qualification session 2 — 2026-09-14

- Still required after supported-host evidence exists: an operator runbook with observed systemd status, protected state checks, real collection/upload, controller outage/restart recovery, binary upgrade, rollback, and enrollment-to-host-adoption evidence.
- No end-user documentation changed in this blocked sandbox checkpoint; `docs/agent/linux-agent-service.md` remains accurate for the current `unit-verified` boundary.


## C3-03 runtime qualification session 3 — 2026-09-14

- No end-user documentation changed because the supported-host runtime boundary remains unavailable. `docs/agent/linux-agent-service.md` remains accurate.
- Still required after host evidence exists: a supported-platform runbook with observed systemd status, protected state, real collection/upload, controller outage/restart recovery, binary upgrade, rollback, and enrollment-to-host-adoption evidence.

## C3-03 runtime qualification session 4 — 2026-09-14

- No end-user documentation changed because the supported-host runtime boundary remains unavailable; `docs/agent/linux-agent-service.md` remains accurate for the current `unit-verified` boundary.
- Still required after host evidence exists: a supported-platform runbook with observed systemd status, protected state, real collection/upload, controller outage/restart recovery, binary upgrade, rollback, and enrollment-to-host-adoption evidence.
- Reusable test-environment note: the full backend suite may fill the 512 MiB `/tmp` tmpfs with disposable `vt-p1`/`voidtower` SQLite and WAL fixtures; cleanup of only those generated patterns restored the full gate without source changes.


## M1-04 model proxy closure — 2026-09-14

- Delivered: `docs/api.md` documents fail-closed model mutation POSTs and that `/v1/chat/completions` does not implicitly switch/reload models.
- Future: add a runtime operator note for selecting/loading a model through the eventual canonical operation adapter, and document the observed local llama.cpp upstream endpoint once a supported runtime qualification exists.
- Current blocker: no live llama.cpp provider/runtime was available in the sandbox, so no runtime serving or release support claim is permitted.


## M1-04 local-host mutation closure — 2026-09-14

- Added to `docs/api.md`: filesystem/plugin/repository-mod mutation routes now fail closed until canonical adapters exist; read-only/status paths remain distinct; mod source configuration is not repository application.
- Future documentation required: operator guidance for the eventual canonical filesystem/plugin/mod action adapters, including plan, approval, job, audit, recovery, and provider qualification behavior.
- Reusable checks: `cd backend && cargo test operations::registry::tests --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`.


## M1-04 deferred mutation ledger — 2026-09-14

- End-user API documentation updated: WireGuard peer mutations and storage mutations now explicitly state the authenticated `503 feature_unavailable` boundary and distinguish read-only status projections.
- Future documentation required: operator guidance for the eventual canonical WireGuard/storage/service/local-VM/LXC adapters, including typed plans, policy/approval, durable jobs, audit/events, recovery, and provider qualification.
- Reusable checks: `cd backend && cargo test operations::registry::tests::deferred_mutation --all-features`; then `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `git diff --check`.


## M1-04 system restart closure — 2026-09-14

- End-user API documentation changed: `docs/api.md` now states that `POST /api/system/restart` authenticates owner/admin sessions and returns `503 feature_unavailable` until a canonical system lifecycle adapter exists; direct restart script/process/signal behavior is not available.
- Future documentation required: operator guidance for the eventual canonical system lifecycle adapter, including immutable plan, policy/approval, durable job, audit/event, restart recovery, and runtime qualification evidence.
- Reusable checks: `cd backend && cargo test operations::registry::tests --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`.


## M1-04 AI process unload closure — 2026-09-14

- End-user API documentation changed: `docs/api.md` documents that `/api/ai/llama/unload` authenticates owner/admin sessions and returns `503 feature_unavailable` without signaling host processes.
- Future documentation required: operator guidance for the eventual canonical AI process lifecycle adapter, including immutable plan, policy/approval, durable job, audit/event, recovery, and runtime qualification behavior.
- Reusable checks: `cd backend && cargo test operations::registry::tests --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`.


## M1-04 interactive shell closure — 2026-09-14

- End-user API documentation updated: `docs/api.md` marks container exec and local/SSH interactive shell routes unavailable, documents authentication-first `503 feature_unavailable`, and distinguishes read-only logs/session metadata from shell execution.
- Future documentation required: canonical shell action/resource contract, command bounds and approval policy, durable job/audit/event/recovery behavior, SSH host-key and credential policy, and runtime qualification on named container/host/SSH environments.
- Reusable checks: `cd backend && cargo test operations::registry::tests --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`.


## M1-04 model/provider lifecycle closure — 2026-09-14

- End-user API documentation updated: `docs/api.md` now states that model download/delete/load, llama/Ollama configuration, Ollama pull, and Ollama create authenticate first and return bounded `503 feature_unavailable` until canonical model/resource adapters exist; it distinguishes read-only status and inference.
- Future documentation required: canonical model resource/action schemas, immutable plans, policy/approval, durable job/idempotency/recovery semantics, path/download bounds, provider credential handling, and named llama.cpp/Ollama runtime qualification.
- Reusable checks: `cd backend && cargo test operations::registry::tests::deferred_mutation --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`.


## M1-04 provider and App Vault boundary closure — 2026-09-14

- End-user API documentation updated: provider credentials are secret references, provider endpoints are bounded/restricted, health failures are redacted, and embed access is owner/admin scoped with same-origin framing.
- Future documentation required: canonical provider/App Vault operation schemas, approval/job/recovery semantics, and named runtime qualification for external providers and Docker-backed embeds.
- Reusable checks: backend focused provider and embed tests; backend full test gate; repository truth; schema ownership; diff check.

## M1-04 provider egress closure — 2026-09-14

- End-user API documentation updated: provider request paths resolve and pin DNS results, reject prohibited address classes, and disable redirects; omitted-field updates revalidate persisted provider settings.
- Future documentation required: operator guidance for controlled provider endpoint allowlisting/egress policy and named runtime qualification against real providers, redirect responses, and DNS-rebinding fixtures.
- Reusable checks: `cd backend && cargo test ai::egress --all-features`; `cd backend && cargo test api::ai_providers::tests --all-features`; `cd backend && cargo test ai::orchestrator::tests --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `git diff --check`.


## M1-04 provider egress closure final — 2026-09-14

- `docs/api.md` now documents DNS resolution/pinning, redirect and proxy behavior, the explicit local-provider exception, and bounded provider failures.
- Future documentation: named runtime runbook for external providers, DNS rebinding fixtures, proxy environment behavior, and Docker/App Vault qualification.

## C3-03 runtime qualification recovery — 2026-09-14

- Tracked documentation changed: `docs/agent/linux-agent-service.md` now has the exact supported-host qualification checklist and states the current sandbox boundary without implying runtime support.
- Still required on a supported host: observed systemd status, protected state permissions, canonical host/adoption before real `/usr/bin/lsblk` collection/upload, outbound-only socket evidence, controller outage/restart recovery, binary upgrade, rollback, and enrollment-to-host-adoption evidence.
- Reusable checks from this recovery: `cd backend && cargo test agent:: --all-features`; the collector, transport, and supervision focused commands; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`.
- Blocker: this sandbox has no `systemctl` or `/run/systemd/private`; clippy and rustfmt components are also absent. Do not promote C3-03 beyond `unit-verified`.

## M1-04 AI Studio generation closure — 2026-09-14

- End-user API documentation changed: `docs/api.md` now documents authenticated-first `503 feature_unavailable` behavior for image generation, TTS, and STT until canonical AI/media operation adapters exist; it distinguishes read-only Studio routes.
- Future documentation required: canonical AI/media resource and action schemas, provider secret/reference policy, bounded media payloads, durable job/idempotency/recovery semantics, and named provider runtime qualification.
- Reusable checks: `cd backend && cargo test operations::registry::tests::deferred_mutation --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`.

## M1-04 deferred compatibility extractor ordering — 2026-09-14

- Deferred filesystem, plugin, service, local LXC/VM, storage, and WireGuard mutation handlers no longer declare `Json`/`Query` request extractors because they cannot execute legacy intent until canonical adapters exist.
- The shared real-router matrix now sends malformed bodies/query data to these route families and proves unauthenticated requests return `401` before extraction; authenticated requests retain bounded `503 feature_unavailable` behavior.
- This is source/real-router integration evidence only. Runtime provider, host, browser, and release qualification remain unavailable; the sandbox has no `systemctl` or `/run/systemd/private`.
- Reusable checks: `cd backend && cargo test operations::registry::tests --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`.

## C3-03 runtime qualification recovery session 6 — 2026-09-14

- No end-user documentation changed because the supported-host runtime remains unavailable; `docs/agent/linux-agent-service.md` still accurately states the unit-verified boundary and qualification checklist.
- Still required after host evidence exists: a supported-platform runbook containing OS/image/version, artifact checksum, observed systemd status, protected state permissions, canonical host/adoption prerequisite, real collection/upload, outbound-only evidence, controller outage/restart recovery, upgrade, rollback, and redacted evidence retention.
- Reusable checks: `cd backend && cargo test agent:: --all-features`; `cd backend && cargo test collector --all-features`; `cd backend && cargo test agent::transport --all-features`; `cd backend && cargo test agent::supervision --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`.
- Blocker: this sandbox has no `systemctl` or `/run/systemd/private`; do not promote C3-03 beyond `unit-verified`.

## C3-03 response-contract recovery — 2026-09-14

- End-user/operator documentation updated: `docs/agent/linux-agent-service.md` now records typed heartbeat/inventory success contracts and fail-closed invalid-state startup behavior.
- Still required: supported-host evidence for persisted pending-snapshot recovery across process restart, outage/ambiguous upload handling, service installation, upgrade, rollback, and enrollment-to-host-adoption.
- Reusable checks: `cd backend && cargo test agent::transport --all-features`; `cd backend && cargo test agent::supervision --all-features`; `git diff --check`.

## C3-03 runtime qualification recovery session 7 — 2026-09-15

- No end-user documentation changed because the supported-host runtime remains unavailable; `docs/agent/linux-agent-service.md` still accurately states the unit-verified boundary and qualification checklist.
- Toolchain recovery installed `rustfmt` and `clippy`, but repository-wide formatting remains blocked by existing drift, including `backend/src/agent/supervision.rs` and `backend/src/agent/transport.rs` plus unrelated files, and Clippy remains blocked by unrelated existing errors; no tracked product documentation or source was changed.
- Still required after host evidence exists: a supported-platform runbook containing OS/image/version, artifact checksum, observed systemd status, protected state permissions, canonical host/adoption prerequisite, real collection/upload, outbound-only evidence, controller outage/restart recovery, upgrade, rollback, and redacted evidence retention.
- Reusable checks: `cd backend && cargo test agent::transport --all-features`; `cd backend && cargo test agent::supervision --all-features`; `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `git diff --check`.
- Blocker: this sandbox has no `systemctl` or `/run/systemd/private`; do not promote C3-03 beyond `unit-verified`.
## C3-03 runtime qualification recovery session 8 — 2026-09-15

- No end-user documentation changed because the supported-host runtime remains unavailable; `docs/agent/linux-agent-service.md` remains accurate for the current `unit-verified` boundary.
- Still required after host evidence exists: a supported-platform runbook containing OS/image/version, artifact checksum, observed systemd status, protected state permissions, canonical host/adoption prerequisite, real collection/upload, outbound-only evidence, controller outage/restart recovery, upgrade, rollback, and redacted evidence retention.
- Reusable checks: `cd backend && cargo test agent::transport --all-features`; `cd backend && cargo test agent::supervision --all-features`; `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features`; `cd backend && cargo test --all-targets --all-features`; `python3 scripts/test_release_gate.py`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`.
- Blocker: PID 1 is Docker's `docker-init`, `systemctl` and the systemd private socket are absent, and the sandbox cannot safely provide a supported host runtime. Do not promote C3-03 beyond `unit-verified`.

## C3-02 agent response-contract completion — 2026-09-15

- End-user documentation updated: `docs/agent/inventory-upload.md` now publishes the complete `InventorySnapshotResultV1` success response and states that malformed or incomplete success responses are retryable upload failures.
- The enrollment contract already documents the shared 512-byte pairing-code limit; no further operator workflow changed.
- Reusable check: `cd backend && cargo test agent::transport --all-features`.
- Future documentation required: supported-host evidence for outage/restart recovery, durable pending snapshots, service installation, upgrade, rollback, and enrollment-to-host-adoption.

## C3-03 durable pending inventory — 2026-09-15

- End-user/operator documentation updated: `docs/agent/linux-agent-service.md` now documents the owner-only `.state.json.pending.json` sidecar, 256 KiB bound, atomic-before-upload ordering, restart reuse, and success-only cleanup.
- Future documentation required: named supported-host evidence for systemd outage/restart recovery, duplicate-safe upload after process restart, upgrade, rollback, and enrollment-to-host-adoption. The sidecar behavior is unit-verified only in this sandbox.
- Reusable checks: `cd backend && cargo test agent::state --all-features`; `cd backend && cargo test agent::supervision --all-features`.

## C3-03 inventory acknowledgement binding — 2026-09-15

- End-user/operator documentation updated: `docs/agent/inventory-upload.md` now states that the response `snapshot_id` must match the uploaded snapshot and that mismatches remain retryable.
- Future documentation required: named supported-host evidence for systemd lifecycle, real collection/upload, outage/restart recovery, upgrade, rollback, and enrollment-to-host-adoption.
- Reusable check: `cd backend && cargo test agent::transport --all-features`.

## C3-03 runtime qualification recovery session 9 — 2026-09-16

- No end-user documentation changed because the required supported-host runtime is unavailable; `docs/agent/linux-agent-service.md` remains accurate and explicitly labels service/release qualification as blocked.
- Still required on a named supported Linux host: systemd installation/status, owner-only state permissions, canonical host adoption before upload, real `/usr/bin/lsblk` collection/upload, outbound-only evidence, controller outage and process-restart pending-snapshot reuse, upgrade, rollback, redacted artifacts, and a checksum.
- Reusable checks: `cd backend && cargo test agent::transport --all-features`; `cd backend && cargo test agent::supervision --all-features`; `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features`; `python3 scripts/test_release_gate.py`; `git diff --check`. Schema ownership remains unverified in this sandbox because `rg` is unavailable (the wrapper exits 0 after its diagnostic).
- Full-gate note: one run reached 610 passing tests but hit SQLite `database is locked` in the existing `cmdb::assets::tests::concurrent_manual_creates_receive_distinct_identifiers`; the 2026-09-16 rerun passed with 611 unit tests, 2 integration tests, and examples. Keep the transient failure in historical evidence, not as a current blocker.
- Blocker: this Docker sandbox has no `systemctl` or `/run/systemd/private`; do not promote C3-03 beyond `unit-verified`.

## C3-03 runtime qualification recovery session 10 — 2026-09-16

- No end-user documentation changed because supported-host systemd and host-device runtime remain unavailable; `docs/agent/linux-agent-service.md` remains accurate.
- The required future operator runbook evidence is unchanged: systemd install/status, owner-only state, canonical host adoption, real collection/upload, outbound-only observation, controller outage/restart recovery, upgrade, rollback, redacted artifacts, and checksum.
- Reusable checks: focused CMDB/enrollment/App Vault/collector/transport/supervision Cargo tests; `cd backend && cargo test --all-targets --all-features` after disposable `/tmp` fixture cleanup; `python3 scripts/test_release_gate.py`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; and `git diff --check`.
- Blockers remain missing `systemctl`/`/run/systemd/private`, pre-existing strict Clippy diagnostics, and missing `rg` for fully trustworthy schema ownership verification.

## C3-03 enrollment/fingerprint contract closure — 2026-09-16

- End-user documentation updated: `docs/agent/node-enrollment.md` now documents the 64 KiB enrollment body cap, stable oversized-body response, and whitespace-normalized snapshot replay identity.
- Future documentation remains required for named supported-host systemd lifecycle, real collection/upload, outage/restart recovery, upgrade, rollback, redacted artifacts, and checksum evidence.
- Reusable checks: `cd backend && cargo test api::node_enroll::tests::enrollment_rejects_oversized_body_with_stable_error --all-features`; `cd backend && cargo test api::cmdb::tests::inventory_upload_is_authenticated_idempotent_and_binds_the_node_host --all-features`; then the full backend, release-gate, repository-truth, schema-ownership, and diff checks.

## C3-03 enrollment and upload boundary hardening — 2026-09-16

- End-user documentation updated: `docs/agent/node-enrollment.md` now documents the 512-byte node-token bound and authentication-before-body-limit behavior.
- Future documentation required: supported-host evidence for service lifecycle, real collection/upload, outage/restart recovery, upgrade, rollback, and enrollment-to-host adoption remains unchanged; no sandbox source test promotes those claims.
- Reusable checks: `cd backend && cargo test agent::transport --all-features`; `cd backend && cargo test api::node_enroll::tests --all-features`; `cd backend && cargo test api::cmdb::tests --all-features`; `python3 scripts/test_release_gate.py`; `git diff --check`.

## C3-03 managed-node authorization contract — 2026-09-17

- End-user documentation updated: `docs/agent/node-enrollment.md` now states that the HTTP `Bearer` scheme is case-insensitive while node credentials remain trimmed and bounded.
- Future documentation remains required for named supported-host systemd lifecycle, real collection/upload, outage/restart recovery, upgrade, rollback, redacted artifacts, and checksum evidence.
- Reusable check: `cd backend && cargo test api::node_enroll::tests --all-features`.

## C3-03 token contract parity — 2026-09-17

- End-user documentation updated: `docs/agent/linux-agent-service.md` now records the shared 512-byte node-token bound and exact-limit behavior.
- Future documentation remains required for named-host systemd installation/status, real collection/upload, outage and restart recovery, upgrade/rollback, redacted artifacts, and checksums; none is claimed from this sandbox.
- Reusable checks: `cd backend && cargo test agent::state --all-features`; `cd backend && cargo test api::node_enroll::tests --all-features`; `cd backend && cargo test agent::supervision --all-features`.

## C3-03 runtime qualification session 11 — 2026-09-17

- No end-user documentation changed because the supported-host runtime boundary remains unavailable; the existing Linux agent and enrollment docs remain accurate and explicitly do not claim runtime qualification.
- Still required after a named host run: observed systemd install/status, owner-only state permissions, canonical host adoption before collection, real collection/upload, outbound-only evidence, controller outage/restart recovery, upgrade, rollback, redacted evidence retention, and artifact checksum.
- Reusable checks from this blocked checkpoint: `python3 scripts/repo_truth.py --repo . --json --check`; the five focused managed-node Cargo commands in `docs/internal/handoffs/2026-09-17-c3-03-runtime-qualification-session-11-blocked.md`; `python3 scripts/test_release_gate.py`; and `git diff --check`.

## C3-03 runtime qualification session 12 — 2026-09-17

- No end-user documentation changed because the supported-host systemd/device boundary remains unavailable. Existing Linux agent, enrollment, and inventory-upload docs remain accurate for the unit/integration-verified boundary and do not claim runtime qualification.
- Still required after a named host run: observed systemd install/status, owner-only state permissions, canonical host adoption before collection, real service-managed `/usr/bin/lsblk` upload, outbound-only evidence, controller outage/restart recovery, upgrade, rollback, redacted evidence retention, and artifact checksum.
- Reusable checks: `python3 scripts/repo_truth.py --repo . --json --check`; `cd backend && cargo test agent::state --all-features`; `cd backend && cargo test api::node_enroll::tests --all-features`; `cd backend && cargo test agent::supervision --all-features`; `cd backend && cargo test agent::transport --all-features`; `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features`; `python3 scripts/test_release_gate.py`; `git diff --check`.

## C3-03 runtime qualification session 13 — 2026-09-17

- No end-user documentation changed. Existing Linux agent, enrollment, and inventory-upload docs remain accurate for the unit/integration-verified boundary and do not claim runtime qualification.
- Still required after a named host run: observed systemd install/status, owner-only state permissions, canonical host adoption, real service-managed `/usr/bin/lsblk` upload, outbound-only evidence, controller outage/restart recovery, upgrade, rollback, redacted evidence retention, and artifact checksum.
- Reusable full backend test invocation: from the repository root, run `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; the variable is required by the golden-path workflow contract test in this sandbox invocation. This does not replace release-gate, repository-truth, schema-ownership, or diff checks.
- Blockers remain absent host systemd/device runtime, unavailable Clippy/rustfmt components, and unavailable `rg` for fully trustworthy schema ownership verification.

## C3-03 agent prequalification hardening — 2026-09-17


## C3-03 enrollment lifecycle and error contract hardening — 2026-09-18

- End-user documentation updated: `docs/agent/inventory-upload.md` now documents atomic single-use pairing-code behavior under concurrent enrollment, non-consuming WireGuard-unavailable responses, structured enrollment audit details, and generic redacted internal-error envelopes.
- Future documentation remains required for named supported-host systemd installation/status, real service-managed collection/upload, outage/restart recovery, upgrade/rollback, redacted evidence retention, and artifact checksums.
- Reusable focused checks: `cd backend && cargo test api::node_enroll::tests::explicit_false_enrolls_without_wireguard_state --all-features`; `cd backend && cargo test api::node_enroll::tests::concurrent_enrollment_claims_a_pairing_code_once --all-features`; `cd backend && cargo test error::tests::internal_failures_return_bounded_redacted_envelopes --all-features`.
- Retrospective: the atomic claim was already implemented but lacked concurrent public-seam evidence; persisted audit metadata should be structured rather than delimiter-built; sandbox runtime and toolchain blockers remain unchanged.
- End-user documentation updated: `docs/agent/linux-inventory-collector.md` now reflects that the fixed `/usr/bin/lsblk` command is invoked by supervision, snapshots are persisted/uploaded through the enrolled node path, and serialized inventory requests are capped at 256 KiB.
- Future documentation required: the supported-host runbook still needs observed systemd installation/status, owner-only state, real service-managed collection/upload, outage/restart recovery, upgrade, rollback, redacted artifacts, and checksum evidence.
- Reusable checks: `cd backend && cargo test agent --all-features`; `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `cargo fmt --check`; and `git diff --check`. The full command must set `GITHUB_WORKSPACE` for the workflow-contract fixture in this sandbox.

## C3-03 enrollment persistence and audit-boundary hardening — 2026-09-18

- End-user documentation updated: `docs/agent/node-enrollment.md` now specifies atomic claim-plus-node persistence, retryability after persistence failure, and structured JSON audit details for enrollment and deletion.
- Future documentation remains required for named supported-host systemd installation/status, service-managed collection/upload, outage/restart recovery, upgrade/rollback, redacted evidence retention, and artifact checksums.
- Reusable focused check: `cd backend && cargo test api::node_enroll::tests --all-features`.
- Blockers: no supported systemd/device runtime, `cargo-fmt` unavailable for the active toolchain, and schema ownership cannot be fully trusted while the wrapper's `rg` dependency is absent.

## C3-03 supported-host runtime qualification session 14 — 2026-09-18

- No end-user documentation changed because the required systemd/device runtime remains unavailable. Existing Linux agent, enrollment, and inventory-upload docs remain accurate for the current unit/integration-verified boundary and do not claim runtime qualification.
- Still required after a named supported-host run: observed systemd installation/status, owner-only state permissions, canonical host adoption, real service-managed `/usr/bin/lsblk` collection/upload, outbound-only evidence, controller outage/restart recovery, upgrade, rollback, redacted evidence retention, and artifact checksum.
- Reusable checks: `cd backend && cargo test agent::transport --all-features`; `cd backend && cargo test agent::supervision --all-features`; `cd backend && cargo test api::cmdb::tests --all-features`; `cd backend && cargo test api::node_enroll::tests --all-features`; `cd backend && GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features`; `python3 scripts/test_release_gate.py`; `python3 scripts/repo_truth.py --repo . --json --check`; `git diff --cached --check`.
- Blockers: no `systemctl` or `/run/systemd/private`, missing `cargo-fmt`/`cargo-clippy`, and missing `rg` for fully trustworthy schema ownership. The dated handoff is `docs/internal/handoffs/2026-09-18-c3-03-runtime-qualification-session-14-blocked.md`.

## C3-03 agent identity-bound recovery and input contract hardening — 2026-09-18

- End-user documentation updated: `docs/agent/inventory-upload.md` now documents semantic snapshot validation before host lookup, including bounded fields, UUID IDs, control-character rejection, identity bounds, and duplicate-key rejection. `docs/agent/linux-agent-service.md` documents the node-bound pending-sidecar envelope and owner-only custom CA file permissions.
- Future documentation remains required for named supported-host systemd installation/status, service-managed collection/upload, outage/restart recovery, upgrade/rollback, redacted evidence retention, and artifact checksums.
- Reusable focused checks: `cd backend && cargo test agent:: --all-features`; `cd backend && cargo test cmdb::contracts::tests:: --all-features`; `cd backend && cargo test api::cmdb::tests::inventory_upload_rejects_semantically_invalid_snapshots --all-features`; `git diff --check`.
- Retrospective: pending inventory must carry the node identity because a state-file replacement can otherwise reuse a valid old sidecar under a new node; shared contract validation prevents malformed snapshots from reaching host lookup or persistence; custom CA trust roots require owner-only permissions. Runtime/toolchain blockers remain unchanged.

## C3-03 supported-host runtime qualification session 15 — 2026-09-18

- No end-user documentation changed because the supported systemd/device runtime remains unavailable; `docs/agent/linux-agent-service.md` remains accurate and explicitly limits C3-03 to unit/integration evidence.
- Still required on a named supported host: observed `systemd-analyze`/`systemctl` installation and status, owner-only state permissions, canonical host adoption, real service-managed `/usr/bin/lsblk` collection/upload, outbound-only observation, controller outage and process-restart recovery, upgrade, rollback, redacted evidence retention, and artifact checksum.
- Reusable checks: `cd backend && cargo test agent:: --all-features`; `cd backend && cargo test api::cmdb::tests --all-features`; `cd backend && cargo test cmdb::contracts::tests:: --all-features`; `cd backend && GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features`; `python3 scripts/test_release_gate.py`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; and `git diff --check`. Clean only disposable `/tmp/vt-p1-*` and `/tmp/voidtower-*` fixtures if the 512 MiB tmpfs fills.
- Blockers: no `systemctl`/systemd private socket or `/dev/block`, missing active-toolchain `cargo-fmt`/`cargo-clippy`, and missing `rg` for fully trustworthy schema ownership. Do not promote runtime or release qualification from this sandbox.

## C3-03 Linux agent release-package readiness — 2026-09-18

- End-user documentation updated: `docs/agent/linux-agent-service.md` now documents release archive contents, installer copying of frontend/agent assets, protected agent-directory creation, dual-unit enablement, the post-enrollment start rule, and the corrected environment-based controller unit.
- Still required on a named supported host: observed systemd installation/status, real service-managed collection/upload, outage/process-restart recovery, upgrade/rollback, redacted evidence retention, and checksummed release artifact evidence. The documentation must be extended with observed results once that host run is available.
- Reusable checks: `python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v`; `cd backend && cargo test agent:: --all-features`; `cd backend && GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `git diff --check`.
- Blockers: this sandbox has no systemd private socket or host `/dev`; active rustfmt/Clippy components and `rg` are unavailable. Package/archive behavior is contract-tested only until the supported-host and release gates run.

## C3-03 Linux agent release-package hardening — 2026-09-18

- End-user documentation updated: `docs/agent/linux-agent-service.md` now states that release checksum absence fails closed, source/catalog tarballs receive the same traversal/member-type validation, and systemd availability requires a live manager probe.
- Retrospective: independent review found and the implementation fixed unsafe source/catalog tar extraction, matrix checksum manifest collisions, false-positive systemd detection, version-drifting fallback from a requested release to `main`, v-prefixed release filename mismatch, unsupported armv7 advertisement, incomplete documented unit guard, offline network fallback, checksum case/duplicate handling, offline MCP pre-cache, reset agent ordering, skip-systemd lifecycle inconsistency, and ignored catalog extraction failures. The reusable archive seam is `scripts/install.sh::validate_tar_archive`; the release workflow now computes one basename-compatible manifest from the downloaded archives and runs the package contract gate before builds.
- Verified: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v` (19 passed); `bash -n scripts/install.sh scripts/build-release.sh`; JSON validation; `git diff --check`. Full Rust tests, repository truth, hygiene, schema ownership, formatting, Clippy, supported-host runtime, and release artifact publication remain subject to the limitations recorded in the prior entry.
- Next documentation required: supported-host installation/status, service-managed collection/upload, outage/process-restart recovery, upgrade/rollback, redacted evidence retention, and checksummed release artifact evidence. Do not promote runtime or release qualification from this sandbox.

## C3-03 collector/CMDB contract parity — 2026-09-18T14:57:30Z

- End-user documentation updated: `docs/agent/linux-inventory-collector.md` and `docs/agent/inventory-upload.md` now document the `TRAN`→`attributes.protocol`, `ROTA`→`attributes.rotation`, serial/WWN attribute-plus-identity mapping, and collector-side shared snapshot validation.
- Future documentation remains required for named supported-host systemd installation/status, service-managed collection/upload, outage/process-restart recovery, upgrade/rollback, redacted evidence retention, and artifact checksums.
- Reusable checks: `cd backend && cargo test collector::tests --all-features`; `cd backend && cargo test api::cmdb::tests::linux_collector_snapshot_reaches_reconciliation_classification --all-features`; then the full backend, release-gate, repository-truth, schema-ownership, and diff checks.
- Retrospective: a sanitized fixture can pass parser tests while still being rejected by downstream identity normalization if its WWN is not valid evidence; the end-to-end fixture now uses a valid non-zero 16-hex WWN and proves trusted registration through the real router/database boundary.

## C3-03 collector/CMDB capacity vocabulary parity — 2026-09-18

- End-user documentation updated: `docs/agent/linux-inventory-collector.md` and `docs/agent/inventory-upload.md` now define `lsblk SIZE` as `attributes.capacity_bytes` and explicitly exclude the `size_bytes` alias.
- Future documentation remains required for named supported-host systemd installation/status, service-managed collection/upload, outage/process-restart recovery, upgrade/rollback, redacted evidence retention, and artifact checksums.
- Reusable focused checks: `cd backend && cargo test collector::tests --all-features`; `cd backend && cargo test api::cmdb::tests::linux_collector_snapshot_reaches_reconciliation_classification --all-features`; `cd backend && cargo test api::cmdb::tests::inventory_upload_rejects_semantically_invalid_snapshots --all-features`.
- Retrospective: CMDB normalization already treated `capacity_bytes` as the canonical positive integer field, while the collector emitted `size_bytes`; aligning the producer, rejecting malformed `SIZE` values before upload, and asserting the real-router rejection closes that contract drift without changing unrelated model/storage/media size APIs.

## C3-03 supported-host runtime qualification session 18 — 2026-09-18

- No end-user documentation changed because the required systemd/device runtime remains unavailable; `docs/agent/linux-agent-service.md` remains accurate for the current unit/integration-verified boundary and does not claim runtime qualification.
- Still required on a named supported host: observed systemd installation/status, owner-only state permissions, canonical host adoption, real service-managed `/usr/bin/lsblk` collection/upload, outbound-only observation, controller outage and process-restart recovery, upgrade, rollback, redacted evidence retention, and artifact checksum.
- Reusable checks: `cd backend && cargo test agent:: --all-features`; `cd backend && cargo test api::cmdb::tests --all-features`; `cd backend && cargo test cmdb::contracts::tests:: --all-features`; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v`; `cd backend && GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; `git diff --check`. Clean only disposable `/tmp/vt-p1-*` and `/tmp/voidtower-*` fixtures if the 512 MiB tmpfs fills.
- Blockers: no `systemctl`/systemd private socket or `/dev/block`, missing active-toolchain `cargo-fmt`/`cargo-clippy`, and missing `rg` for fully trustworthy schema ownership. Do not promote runtime or release qualification from this sandbox.

## C3-03 supported-host runtime qualification session 19 — 2026-09-18

- No end-user documentation changed because the required systemd/device runtime remains unavailable; `docs/agent/linux-agent-service.md` remains accurate for the current unit/integration-verified boundary and does not claim runtime qualification.
- Future end-user/operator documentation remains required after a named supported-host run: observed systemd installation/status, owner-only state permissions, canonical host adoption, real service-managed `/usr/bin/lsblk` collection/upload, outbound-only observation, controller outage and process-restart recovery, upgrade, rollback, redacted evidence retention, and artifact checksum.
- Reusable checks: `cd backend && cargo test agent:: --all-features`; `cd backend && cargo test api::cmdb::tests --all-features`; `cd backend && cargo test cmdb::contracts::tests:: --all-features`; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v`; `cd backend && GITHUB_WORKSPACE="$PWD/.." cargo test --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; and `git diff --check`.
- Blockers: no `systemctl`/systemd private socket or `/dev/block`, missing active-toolchain `cargo-fmt`/`cargo-clippy`, and missing `rg` for fully trustworthy schema ownership. Do not promote runtime or release qualification from this sandbox.

## C3-03 supported-host runtime qualification session 20 — 2026-09-19

- No end-user documentation changed because the required systemd/device runtime remains unavailable; `docs/agent/linux-agent-service.md` remains accurate for the current unit/integration-verified boundary and does not claim runtime qualification.
- Future end-user/operator documentation remains required after a named supported-host run: observed systemd installation/status, owner-only state permissions, canonical host adoption, real service-managed `/usr/bin/lsblk` collection/upload, controller outage and process-restart recovery, upgrade, rollback, redacted evidence retention, and artifact checksum.
- Reusable checks: `cd backend && cargo test agent:: --all-features`; `cd backend && cargo test api::cmdb::tests --all-features`; `cd backend && cargo test cmdb::contracts::tests:: --all-features`; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v`; `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `scripts/check-schema-migration-ownership.sh`; and `git diff --check`.
## C3-03 supported-host runtime qualification session 21 — 2026-09-19

- No end-user documentation changed because the required systemd/device runtime remains unavailable; `docs/agent/linux-agent-service.md` remains accurate for the current unit/integration-verified boundary and does not claim runtime qualification.
- Future end-user/operator documentation remains required after a named supported-host run: observed systemd installation/status, owner-only state permissions, canonical host adoption, real service-managed `/usr/bin/lsblk` collection/upload, outbound-only observation, controller outage and process-restart recovery, upgrade, rollback, redacted evidence retention, and artifact checksum.
- Reusable checks: `cd backend && cargo test agent:: --all-features`; `cd backend && cargo test api::cmdb::tests --all-features`; `cd backend && cargo test cmdb::contracts::tests:: --all-features`; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v`; `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `bash -n scripts/install.sh scripts/build-release.sh`; `python3 -m json.tool scripts/release-gates.json`; `git diff --check`; and `scripts/check-schema-migration-ownership.sh` (blocked here because `rg` is unavailable).
- Blockers remain absent `systemctl`/systemd private socket and `/dev/block`, missing active-toolchain `cargo-fmt`/`cargo-clippy`, and missing `rg` for fully trustworthy schema ownership. Do not promote runtime or release qualification from this sandbox.

## C3-03 executable recovery and collector failure contracts — 2026-09-19

- End-user/operator documentation updated: `docs/agent/linux-agent-service.md` now names non-zero `lsblk` exit, non-UTF-8 output, stderr overflow, empty/malformed output, and timeout as bounded collection failures; timed-out processes are killed before retry.
- Future documentation required: a named supported Linux run must record actual systemd status, owner-only state permissions, service-managed `/usr/bin/lsblk` collection/upload, controller outage and process-restart recovery, upgrade, rollback, and artifact checksum. The current loopback integration test is not a runtime qualification substitute.
- Reusable checks: `cd backend && cargo test collector::tests --all-features`; `cd backend && cargo test agent:: --all-features`; `cd backend && cargo test api::cmdb::tests --all-features`; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v`; `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `bash -n scripts/install.sh scripts/build-release.sh`; `python3 -m json.tool scripts/release-gates.json`; and `git diff --check`.
- Remaining blockers: no systemd private socket or `/dev/block` in this sandbox; `cargo-fmt` and `cargo-clippy` are absent from the active Rust toolchain; and the schema ownership wrapper cannot be trusted while `rg` is unavailable. A first concurrent full-gate attempt also exhausted the 512 MiB `/tmp` disposable-fixture space; serial rerun after removing only documented `/tmp/vt-p1-*` and `/tmp/voidtower-*` fixtures passed.

## C3-03 supported-host runtime qualification session 23 — 2026-09-19

- No end-user documentation changed because the supported systemd/device runtime remains unavailable; `docs/agent/linux-agent-service.md` remains accurate for the current unit/integration-verified boundary.
- Required after a named supported-host run: redacted evidence for systemd installation/status, owner-only state permissions, canonical host adoption, real service-managed `/usr/bin/lsblk` collection/upload, outbound-only behavior, controller outage and process-restart recovery, upgrade, rollback, and artifact checksum.
- Reusable checks: `cd backend && cargo test agent:: --all-features`; `cd backend && cargo test api::cmdb::tests --all-features`; `cd backend && cargo test cmdb::contracts::tests:: --all-features`; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v`; `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `bash -n scripts/install.sh scripts/build-release.sh`; `python3 -m json.tool scripts/release-gates.json`; and `git diff --check`.
- Blockers: no `systemctl`/systemd private socket or `/dev/block`; missing active-toolchain `cargo-fmt`/`cargo-clippy`; missing `rg` for fully trustworthy schema ownership; and pre-existing repository-hygiene policy failures. Do not promote runtime or release qualification from this sandbox.

## C3-03 state recovery integrity hardening — 2026-09-19

- End-user/operator documentation updated: `docs/agent/linux-agent-service.md` now states the Unix parent-chain and owner-match checks applied before state or pending-sidecar recovery.
- Future documentation required: a named supported-host run must still record actual systemd installation/status, owner-only state permissions, service-managed collection/upload, controller outage/process-restart recovery, upgrade, rollback, and artifact checksum.
- Reusable check: `cargo test --manifest-path backend/Cargo.toml agent::state::tests:: --all-features`.
- Limitation: this source hardening is unit-verified only; runtime/release qualification remains blocked by unavailable systemd, host `/dev`, and packaged-artifact boundaries.

## C3-03 enrollment input and diagnostic hardening — 2026-09-19

- End-user documentation changed: `docs/agent/node-enrollment.md` now documents `--pairing-code-stdin`, its newline-terminated one-line/512-byte contract, first-line-only consumption, and the argv exposure limitation of legacy `--pairing-code`; `docs/agent/linux-agent-service.md` now states that special CA paths fail promptly under nonblocking Unix opens.
- Future documentation required: supported-host evidence must still record systemd installation/status, protected state permissions, service-managed collection/upload, outage/process-restart recovery, upgrade, rollback, and artifact checksum; this source-only hardening does not advance C3-03 runtime/release maturity.
- Reusable checks: `cargo test --manifest-path backend/Cargo.toml agent::tests --all-features`; `cargo test --manifest-path backend/Cargo.toml lifecycle_tests --all-features`; `cargo test --manifest-path backend/Cargo.toml error::tests --all-features`; `cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; and `git diff --check`.
- Limitation: the compatibility `--pairing-code VALUE` path remains process-argv visible by design until a future breaking CLI change; operational guidance must use stdin.

## C3-03 enrollment input and diagnostic hardening handoff — 2026-09-19

- End-user documentation changed: `docs/agent/node-enrollment.md` recommends `--pairing-code-stdin` and documents its one-line/512-byte/UTF-8 behavior, while `docs/agent/linux-agent-service.md` documents fail-fast handling for special CA paths and redacted diagnostics.
- Future documentation required: the supported-host evidence runbook must record systemd installation/status, protected state permissions, service-managed collection/upload, controller outage and process-restart recovery, upgrade, rollback, and artifact checksum. No source-only test substitutes for that runbook.
- Reusable checks: `cargo test --manifest-path backend/Cargo.toml agent:: --all-features`; `cargo test --manifest-path backend/Cargo.toml lifecycle_tests --all-features`; `cargo test --manifest-path backend/Cargo.toml error::tests --all-features`; `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_agent_package scripts.test_release_gate -v`; `python3 scripts/repo_truth.py --repo . --json --check`; shell/JSON syntax checks; and `git diff --check`.
- Limitation: runtime/release qualification remains blocked by the Docker sandbox's absent systemd/private socket/host device boundary; strict Clippy/rustfmt and trustworthy schema ownership are also unavailable here.

## API-token cache invalidation hardening — 2026-09-19

- End-user documentation changed: `docs/api-tokens.md` now states that revocation removes the database row, invalidates the in-memory compatibility session, and returns `401 Unauthorized` on the next Bearer request; token expiry is checked independently from the temporary session lifetime.
- Reusable focused check: `cargo test --manifest-path backend/Cargo.toml scope_bypass_tests --all-features`.
- Future documentation remains required for the supported-host C3-03 systemd/device/release runbook; this authentication hardening does not promote that blocked runtime boundary.
- Evidence status: integration-verified on the real Axum router and full-backend test target; source truth, diff hygiene, and independent security/logic review passed. `cargo fmt --check` and strict Clippy remain blocked because the active Rust toolchain lacks `cargo-fmt` and `cargo-clippy`.

## C3-02 inventory convergence safety — 2026-09-19

- End-user documentation changed: `docs/agent/inventory-upload.md` now distinguishes valid host-only uploads from non-empty convergence, documents `missing: 0` for empty entity sets, and states that inventory audit/events share correlation and node attribution.
- Future documentation remains required for the named supported-host C3-03 runbook: systemd installation/status, protected state permissions, service-managed collection/upload, outage and process-restart recovery, upgrade, rollback, and artifact checksum. This source/real-router slice does not promote runtime or release maturity.
- Reusable focused check: `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml api::cmdb::tests --all-features`; final applicable checks must also include full backend, repository truth, and diff hygiene.

## M1-04 App Vault response and proxy contract hardening — 2026-09-19

- End-user API documentation changed: `docs/api.md` now documents owner-scoped App Vault reads, the 256 KiB redacted compose response, 64 KiB UTF-8-safe logs, bounded and timed Docker command failures and external discovery metadata, safe DTOs without host paths, validated `open-ui` URL construction, and the 4 MiB query-preserving embed proxy contract.
- Future documentation required: runtime Docker/App Vault/browser qualification and the eventual canonical durable operation schemas for deploy/adopt/compose mutations.
- Reusable checks: `cargo test --manifest-path backend/Cargo.toml api::apps --all-features`; `npm run type-check`; `git diff --check`.
- Limitation: this slice has real-router/loopback evidence but no host Docker/browser runtime qualification; C3-03 supported-host systemd qualification remains separately blocked.

## M1-04 App Vault review remediation — 2026-09-20

- End-user API documentation changed: `docs/api.md` now documents that `/api/members/me/access` omits drive host paths and that Docker status failures remain visible as bounded errors instead of being rendered as an empty successful status.
- Future documentation required: runtime Docker/App Vault/browser qualification and the eventual canonical durable operation schemas for deploy/adopt/compose mutations.
- Reusable checks: `cargo test --manifest-path backend/Cargo.toml api::apps::security_tests:: --all-features`; `cargo test --manifest-path backend/Cargo.toml containers::log_output_tests --all-features`; `cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `(cd frontend && npm test && npm run type-check && npm run build && npm run lint)`; `python3 scripts/repo_truth.py --repo . --json --check`; release-gate/schema/shell/JSON checks; and `git diff --check`.
- Review remediation evidence: inline command/header redaction and kill-on-overflow behavior are unit-tested. Runtime/browser/Docker qualification remains blocked; `cargo-fmt`/`cargo-clippy` are unavailable in the active toolchain, and the schema wrapper reports missing `rg` despite its exit status.

## M1-04 App Vault final trust-boundary hardening — 2026-09-20

- Public API documentation should retain the rule that authorization precedes JSON validation on App Vault/member-management body routes; malformed unauthenticated requests receive auth errors before parser or host-path behavior.
- Future documentation required: explain process-group cancellation semantics and the distinction between bounded loopback/integration evidence and host Docker qualification. Canonical durable operation schemas for deploy/adopt/compose mutations remain a separate milestone.
- Reusable checks: `cargo test --manifest-path backend/Cargo.toml containers::log_output_tests --all-features`; `cargo test --manifest-path backend/Cargo.toml api::apps::security_tests:: --all-features`; `GITHUB_WORKSPACE="$PWD" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `python3 scripts/repo_truth.py --repo . --json --check`; `python3 scripts/test_release_gate.py -v`; and `git diff --check`.
- Environment limitations: direct release-gate execution is not release-qualified here because `/tmp` can exhaust the 512 MiB sandbox tmpfs during parallel SQLite/frontend output, `cargo-clippy`/`cargo-deny`/rustfmt are unavailable, and unrelated worktree changes are preserved. Runtime/browser/Docker qualification remains blocked.

## R0-02 release-candidate gate runner — 2026-09-20

- End-user/developer documentation added: `docs/release-gates.md` documents the `scripts/release_gate.py` commands, changed-versus-all scope behavior, exit statuses, explicit-argv and repository-boundary rules, redaction, bounded diagnostics, artifact hashes, and evidence maturity limits. `CONTRIBUTING.md` links the focused command from the review workflow.
- Future documentation required: a release-operator runbook must add the exact candidate artifact build, installation, startup, upgrade, backup/restore or rollback, named-platform runtime, and publication evidence once those environments and prerequisites are available.
- Reusable checks: `python3 scripts/test_release_gate.py -v`; `python3 scripts/test_repo_truth.py -v`; `python3 scripts/repo_truth.py --repo . --json --check`; `python3 scripts/release_gate.py --repo . --scope changed --json`; and `git diff --check`.
- Evidence boundary: this source/test slice is unit-verified for the runner and source-truth contracts; it is not runtime-verified or release-qualified. Full release-gate execution remains blocked by pre-existing hygiene findings and unavailable `cargo-deny`/host runtime prerequisites.
- Follow-up documentation: `docs/release-gates.md` now documents the parent-death-aware process supervisor and forked-descendant cleanup contract; keep the eventual operator runbook aligned with the exact platform-specific process boundary.
- Reusable follow-up evidence: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_release_gate.ReleaseGateCliTests.test_runner_death_kills_forked_gate_descendants -v` proves Linux runner-death cleanup, while the full focused suites cover AWS underscore/hyphen credential redaction. This remains source/unit evidence, not runtime or release qualification.

## R0-02 repository prerequisite closure — 2026-09-20

- End-user/developer documentation changed: `docs/release-gates.md` and `CONTRIBUTING.md` now document the repository prerequisite commands, the tracked `docs/internal` continuity policy, and the schema gate's no-`rg`, contiguous-migration, and fail-closed behavior.
- Future documentation required: the release-operator runbook still needs candidate artifact build, clean install, first start, upgrade, backup/restore or rollback, named-platform runtime, and publication evidence. These are not established by source/governance checks.
- Reusable checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_repository_prerequisites -v` (9 tests); `bash scripts/check-repository-hygiene.sh`; `bash scripts/check-schema-migration-ownership.sh`; and `git diff --check`. The prerequisite suite includes unusual-path, symlink, SQLx turbofish/macro, query-file, and canonical-containment adversarial fixtures.
- Evidence boundary: the governance changes are unit-verified and the real checkout's repository hygiene/schema gates are integration-verified at the shell/Git boundary; `cargo deny check` is green after the lockfile security update. Full backend tests are green, while strict Clippy/formatting expose pre-existing unrelated findings. Runtime, browser, Docker, installation, upgrade, recovery, and release qualification remain separate gaps.

## R0-03 backend quality prerequisite closure — 2026-09-20

- Developer documentation changed: `docs/release-gates.md` now names `backend-format` as a required manifest gate and requires it to pass alongside strict Clippy; the dated handoff records the exact all-subsystem evidence and dependency setup.
- No end-user behavior changed. Future operator documentation still needs the produced artifact, clean installation, first startup, upgrade, backup/restore or rollback, named-platform runtime, and publication runbook; this source/build milestone cannot establish those claims.
- Reusable checks: `cd backend && cargo fmt --all -- --check`; `cd backend && cargo clippy --all-targets --all-features -- -D warnings`; `GITHUB_WORKSPACE=\"$PWD\" cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `cargo deny check`; `PYTHONPATH=/workspace/.voidtower-mcp-deps python3 -m unittest discover -s odysseus-mcp-servers/tests -v`; and `PYTHONPATH=/workspace/.voidtower-mcp-deps PYTHONDONTWRITEBYTECODE=1 python3 scripts/release_gate.py --repo . --scope all --json --output dev-data/release-gate-r0-03-final-qualified.json`.
- Evidence status: `integration-verified` at the repository/build/test boundary. Runtime, browser, Docker, clean-install, upgrade/recovery, named-platform, and release qualification remain blocked by unavailable host/runtime boundaries.

## M1-02 built-in MCP and Studio ingress boundary — 2026-09-20

- End-user API documentation changed: `docs/api.md` now documents enabling and authenticating built-in MCP, the JSON-RPC 2.0 request/error contract, 64 KiB bounds, the `container.start` scope and durable-job semantics, Studio's shared invocation boundary, and the fact that `/api/ai/ask` does not execute mutation tools.
- Future documentation required: standalone MCP adapter parity is documented separately; the remaining M1-02 family still needs stable adapter contracts for webhook/automation/scheduler/CLI ingress, approval/recovery examples for ambiguous outcomes, and a supported runtime/provider qualification runbook.
- Reusable focused checks: `cd backend && cargo test api::scope_bypass_tests --all-features`; `cd backend && cargo test api::mcp::tests --all-features`; `cd backend && cargo test api::studio::tests --all-features`; `cd backend && cargo test auth::scope_enforce --all-features`; `cd backend && cargo test api::ai_ask::tests --all-features`.
- Evidence boundary: real-router source/test integration is verified for this bounded MCP/Studio/AI contract. External provider execution, browser/runtime qualification, packaged installation, upgrade/recovery, and release qualification remain blocked or untested.

## M1-02 ingress validation correction — 2026-09-20

- End-user API documentation was refined in `docs/api.md`: unauthenticated malformed MCP bodies are rejected before parsing; authenticated malformed JSON is `400` with a JSON-RPC error; invalid id types and unknown fields use typed `422` validation; non-object initialize/tools/list params and explicit null tool arguments return JSON-RPC `-32602`; Studio's standard validation envelope is documented.
- Developer continuity now records strict `tools/call` params, direct-tool schemas, shared error redaction, and the 4096-character tool-error bound. Future documentation still needs standalone MCP adapter parity, webhook/automation/scheduler/CLI ingress contracts, ambiguous-outcome recovery examples, and supported runtime/provider qualification.
- Reusable final checks: `cd backend && cargo test api::scope_bypass_tests --all-features`; `cd backend && cargo test api::mcp::tests --all-features`; `cd backend && cargo test api::studio::tests --all-features`; `cd backend && cargo test auth::scope_enforce --all-features`; `cd backend && cargo test api::ai_ask::tests --all-features`; `cd backend && cargo clippy --all-targets --all-features -- -D warnings`; `cd backend && cargo fmt --all -- --check`; and `cd backend && cargo test --all-targets --all-features`.

## M1-02 final review correction — 2026-09-20

- End-user documentation now records Studio MCP's preserved extractor statuses: `400` malformed JSON, `415` unsupported media, `413` oversized body, and `422` typed validation/unknown fields. No new end-user guide is required for this correction beyond the API contract.
- Final reusable evidence is `cd backend && cargo test --all-targets --all-features` (679 unit tests and 2 golden-path integration tests), strict Clippy, rustfmt check, schema migration ownership, `git diff --check`, and source-truth check. Runtime/browser/provider qualification remains future documentation work.

## M1-03 automation/webhook ingress contract hardening — 2026-09-20

- End-user documentation changed: `docs/api.md` now documents operator-only automation reads, bounded run-history limits, strict create/update fields, timeout/name/command/description bounds, supported schedule forms, and canonical durable-job acceptance. `docs/integrations/odysseus.md` now documents the durable `automation_id` webhook contract, exact intent rules, dry-run, idempotency replay/conflict, and deferred service actions.
- Developer continuity changed: `docs/internal/agent-knowledge/system-map.md` records the automation/webhook trust boundary, canonical actor/ingress semantics, and evidence class. No migration or schema ownership change was needed.
- Future documentation required: signed inbound webhook requests with replay protection, outbound webhook URL/egress security, a canonical service-action adapter, CLI convergence, and provider/runtime/restart qualification remain open M1-03/M1-04 work.
- Reusable focused checks: `cd backend && cargo test api::operation_workflows_tests --all-features -- --nocapture`; `cd backend && cargo test api::integrations::tests --all-features -- --nocapture`; `cd backend && cargo fmt --all -- --check`; `cd backend && cargo clippy --all-targets --all-features -- -D warnings`; and `git diff --check`.
- Evidence boundary: integration-verified at the real Axum-router/SQLite boundary only. No host runtime, Docker provider, browser, packaged artifact, or release qualification was performed.

## M1-03 signed inbound webhook verification and replay protection — 2026-09-20

- End-user documentation changed: `docs/api.md`, `docs/integrations/odysseus.md`, and `frontend/src/pages/Integrations.tsx` now publish the timestamp/nonce/raw-body HMAC contract, ±300-second clock window, nonce grammar, 15-minute receipt retention, bounded errors, and the OpenSSL/curl setup shape.
- Future documentation required: rotation failure-atomicity and encrypted secret-manager storage for the webhook credential, outbound webhook URL/egress hardening, canonical service-action adapters, CLI convergence, and named provider/runtime/restart qualification.
- Reusable checks: `cd backend && cargo test api::operation_workflows_tests --all-features -- --nocapture`; `cd backend && cargo test db::tests --all-features -- --nocapture`; `cd backend && cargo fmt --all -- --check`; `cd backend && cargo clippy --all-targets --all-features -- -D warnings`; `cd frontend && npm test -- --passWithNoTests`; `cd frontend && npm run type-check`; `cd frontend && npm run lint`; `cd frontend && npm run build`; `bash scripts/check-schema-migration-ownership.sh`; and `git diff --check`.
- Evidence boundary: integration-verified at the real Axum-router/SQLite boundary plus frontend build/test gates. Runtime, Docker, browser, installation, upgrade/recovery, and release qualification remain unavailable in this sandbox.

## M1-03 encrypted inbound webhook credential continuation — 2026-09-20

- End-user documentation changed: `docs/api.md`, `docs/integrations/odysseus.md`, and the Integrations page now document encrypted-at-rest storage, one-time reveal on explicit regeneration, GET redaction, and revoke/regenerate lifecycle behavior.
- Future documentation required: supported-host startup migration observation, backup/restore handling for encrypted secret references, outbound webhook URL/egress security, canonical service adapters, and named provider/runtime/restart qualification.
- Reusable checks: `cd backend && cargo test api::secrets::webhook_migration_tests --all-features -- --nocapture`; `cd backend && cargo test api::integrations::tests::odysseus_config_creates_metadata_only_secret_and_revoke_disables_it --all-features -- --nocapture`; `cd backend && cargo test api::operation_workflows_tests --all-features -- --nocapture`; `cd frontend && npm run type-check && npm run lint && npm run build`; and `git diff --check`.
- Evidence boundary: integration-verified at the real Axum-router/SQLite boundary and frontend build/type/lint boundary. Runtime, browser, Docker, installation, upgrade/recovery, and release qualification remain unavailable in this sandbox.

## M1-03 Odysseus outbound URL and egress hardening — 2026-09-20

- End-user documentation changed: `docs/integrations/odysseus.md` now defines the `allowed_url` purpose, HTTP(S)/credential/query/fragment/length validation, explicit empty-value clearing, local-network exception, DNS pinning, proxy and redirect behavior, bounded theme response, and stable error boundary. The Integrations UI label and native panel method now match the backend contract.
- Future documentation required: a supported-runtime runbook with an observed Odysseus service, DNS-rebinding fixture, redirect/oversized-response fixture, and provider outage/restart behavior; browser qualification remains separate from the source/build evidence.
- Reusable checks: `cd backend && cargo test ai::egress::tests --all-features -- --nocapture`; `cd backend && cargo test api::integrations::tests --all-features -- --nocapture`; `cd backend && cargo fmt --all -- --check`; `cd backend && cargo clippy --all-targets --all-features -- -D warnings`; `cd frontend && npm test -- --passWithNoTests`; `cd frontend && npm run type-check`; `cd frontend && npm run lint`; `cd frontend && npm run build`; `bash scripts/check-schema-migration-ownership.sh`; `python3 scripts/repo_truth.py --repo . --json --check`; and `git diff --check`.
- Evidence boundary: integration-verified for backend focused real-router/controlled-upstream checks and frontend type/lint/build gates. Runtime, browser, Docker, installation, upgrade/recovery, and release qualification remain unavailable in this sandbox.

## M1-04 production compatibility mutation inventory — 2026-09-20

- End-user/developer API documentation changed: `docs/api.md` now documents the credential-safe source-boundary command, its classification families, fail-closed unknown-callsite behavior, and the separate generic notification-webhook egress boundary.
- CI now runs `scripts/test_compatibility_mutation_inventory.py` and `scripts/compatibility_mutation_inventory.py --repo . --check` before repository hygiene. The scanner output is suitable for handoff evidence but is source-only, not runtime/provider proof; exact module/function exception identities and source-boundary failures are covered by fixtures.
- Future documentation required: canonical adapters and operator contracts for deferred App Vault/model/service/storage/local-host mutations; generic notification-webhook URL/egress controls; and named provider/runtime/restart qualification. Do not copy the current classified-marker total into docs.
- Reusable checks: `python3 -m unittest scripts.test_compatibility_mutation_inventory -v`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `python3 scripts/repo_truth.py --repo . --json --check`; and `git diff --check`.
- Evidence boundary: unit-verified scanner fixtures plus implemented CI enforcement. No provider runtime, browser, Docker, installation, upgrade/recovery, or release qualification was performed.

## M1-04 parser-backed compatibility inventory continuation — 2026-09-20

- Developer documentation required and added: `docs/api.md` now states that the source check uses `rustfmt` syntax validation, exact token call shapes, alias/module/`impl` identity, immediate `cfg(test)` item handling, and immutable deferred-body evidence.
- Future end-user/operator documentation remains required for the eventual canonical adapters and plan/approval/job/audit/recovery contracts of deferred App Vault, model, service, storage, and local-host mutations. This checker does not make those operations available.
- Future qualification documentation remains required for provider/runtime/browser/Docker/install/upgrade/recovery evidence; this sandbox only establishes source/unit evidence.
- Reusable checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `bash scripts/check-schema-migration-ownership.sh`; `bash scripts/check-repository-hygiene.sh`; and `git diff --check`.

## M1-04 compatibility inventory review blocker retrospective — 2026-09-20

- Learned: a passing lexical inventory is not sufficient proof of a fail-closed Rust mutation boundary; independent review found valid-syntax and exception-spoofing paths beyond the fixtures.
- Verified: the current checkout scan reports zero unknown findings, 13 inventory fixtures and 31 combined Python tests pass, repository truth/migration/hygiene checks pass, frontend checks pass, and backend full-suite evidence is intermittent on an unrelated SQLite lock test.
- Blocked: no commit or release qualification; independent review requires AST/parser-backed exact call-shape, alias, cfg-module, inline-module, and immutable exception evidence.
- Reusable commands: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check`; `bash scripts/check-schema-migration-ownership.sh`; `bash scripts/check-repository-hygiene.sh`; and `git diff --cached --check`.
- End-user documentation changed: `docs/api.md` documents the source-boundary command and evidence limit; future documentation still requires canonical adapters for deferred mutations and a reviewed parser-backed enforcement contract.

## M1-04 parser-backed inventory hardening — 2026-09-20

- Developer documentation updated: `docs/api.md` now records normalized-token digest pinning, exact evidence file/path checks, unsupported-syntax/macro fail-closed behavior, and CI rustfmt provisioning.
- Future end-user/operator documentation remains required for the canonical adapters behind deferred App Vault/model/service/storage/local-host mutations and for generic notification-webhook egress controls. The parser inventory does not qualify provider, host, browser, installation, upgrade/recovery, or release behavior.
- Reusable checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check`; `bash scripts/check-schema-migration-ownership.sh`; `bash scripts/check-repository-hygiene.sh`; and `git diff --check`.
- Evidence boundary: `unit-verified` for the parser fixtures and source check, `implemented` for CI enforcement; no runtime or release claim is allowed from this batch.

## M1-04 parser-resolution adversarial closure — 2026-09-21

- Developer documentation updated: `docs/api.md` now records rejection of unknown wildcard imports, private/module-qualified mutation aliases, bare aliases with unallowlisted provenance, qualified filesystem `File` mutators, async/closure/nested-function-scoped deferred errors, local helper shadowing, local canonical-module spoofing, and allowlisted resolved canonical targets.
- Future developer documentation remains required for compiler-grade Rust module/import/call-shape resolution or a complete unsupported-form contract, plus an independently immutable/reviewed exception-ledger approval workflow. The checkout-local digest remains edit detection only.
- Final independent review blocked commit: broad `crate::operations::`/`crate::networking::` prefixes still permit arbitrary aliases, imported canonical names remain shadowable by locals/parameters, and nested canonical calls can authorize an enclosing mutation. The next slice must close these exact gaps before runtime or release claims.
- Future operator documentation remains required for canonical adapters and plan/approval/job/audit/recovery contracts behind deferred App Vault/model/service/storage/local-host mutations and generic notification-webhook egress controls.
- Reusable checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `python3 -m py_compile scripts/rust_source_parser.py scripts/compatibility_mutation_inventory.py scripts/test_compatibility_mutation_inventory.py`; then schema/hygiene, backend format/Clippy/full tests, repository truth, and diff checks.
- Evidence boundary: focused parser/repository-truth tests and source inventory are unit/source-enforcement evidence only. Provider, runtime, browser, Docker, installation, upgrade/recovery, and release qualification remain unperformed.

## M1-04 parser resolution blocker — 2026-09-20

- Independent review remains blocking: valid raw identifiers, multi-hop aliases, function values, receiver provenance, UFCS/request-builder forms, and executable item-level aliases can still evade a lightweight token scanner.
- Documentation boundary: do not describe the current source check as complete fail-closed Rust coverage. The next slice must adopt compiler-grade AST/module/import resolution or reject every unsupported expression/item form.
- Reusable checks remain the focused 48-test command and source inventory command in the preceding entry; no commit was created and no runtime/provider qualification was attempted.

## M1-04 parser-resolution closure — 2026-09-20

- Developer documentation changed: `docs/api.md` now names raw-identifier and multi-hop alias resolution plus the fail-closed boundary for mutation function values, request-builder receiver aliases, unsupported call shapes/macros, and executable item initializers.
- Future documentation remains required for canonical adapters and operator contracts behind deferred App Vault/model/service/storage/local-host mutations, plus generic notification-webhook egress controls. The source inventory still does not make deferred operations available.
- Reusable checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `python3 /workspace/.hermes/profiles/voidtower-dev/skills/software-development/voidtower-dev/scripts/slice_batch.py --repo . --manifest docs/internal/evidence/2026-09-20-m1-04-parser-resolution-closure/batch.json --output docs/internal/evidence/2026-09-20-m1-04-parser-resolution-closure/final-report`; and `git diff --check`.
- Limitation: evidence is unit/source-enforcement only. Provider, runtime, browser, Docker, installation, upgrade/recovery, and release qualification remain unperformed; the next dependency is the tracked M1-04 broader acceptance before V6-01.

## M1-04 parser-resolution closure review blocker — 2026-09-20

- Developer documentation corrected: `docs/api.md` now states that the checker is a bounded token-parser aid and explicitly records the unresolved re-export/module, UFCS/angle-bracket, closure/control-flow, canonical-suffix, and independent-exception-ledger limitations.
- Future documentation required: a reviewed parser contract must define compiler-grade module/import/call-shape coverage or fail-closed rejection, and an independently reviewed exception-ledger workflow must define who may approve and regenerate compatibility exceptions. Only then can the API page describe the source check as an enforcement boundary rather than a bounded aid.
- Reusable checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `bash scripts/check-schema-migration-ownership.sh`; `bash scripts/check-repository-hygiene.sh`; `cd backend && cargo fmt --all -- --check`; `cd backend && cargo clippy --all-targets --all-features -- -D warnings`; and `git diff --check`.
- Blocker: independent review rejects completion despite 55 focused Python tests and a 146/0 source report. No commit or provider/runtime/browser/Docker/install/upgrade/recovery qualification was produced; align staged/worktree parser files and resolve the semantic gaps before the next review.

## M1-04 parser-resolution fail-closed continuation — 2026-09-21

- End-user/developer documentation updated: `docs/api.md` now describes fail-closed rejection of unsafe re-exports, UFCS/angle-bracket mutation calls, closure-local unavailable errors, and suffix-shaped helper spoofing; it documents same-module exact-adapter proof for trusted helpers.
- Future developer documentation remains required for a compiler-grade Rust module/import/call-shape resolver or a complete unsupported-form contract, plus an independently immutable/reviewed exception-ledger approval workflow. The current checkout-local digest remains edit detection only.
- Future operator documentation remains required for canonical adapters and plan/approval/job/audit/recovery contracts behind deferred App Vault/model/service/storage/local-host mutations and for generic notification-webhook egress controls.
- Reusable checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -v`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `python3 -m py_compile scripts/rust_source_parser.py scripts/compatibility_mutation_inventory.py scripts/test_compatibility_mutation_inventory.py`; then schema/hygiene, backend, and diff gates.
- Evidence boundary: `unit-verified` for the 59 parser/repository-truth tests and source check; no provider, runtime, browser, Docker, installation, upgrade/recovery, or release qualification was performed.

## M1-04 parser-resolution canonical scope closure — 2026-09-21

- Developer documentation updated: `docs/api.md` now documents exact canonical adapter identities, rejection of broad operations/networking/CMDB/support provenance, imported-name shadow handling across common Rust patterns, nested-function/closure/async-block item-scope isolation, typed filesystem receiver markers, and external `cfg(test)` module exclusion.
- Future developer documentation remains required for compiler-grade Rust module/import/call-shape resolution or a complete unsupported-form contract, plus an independently immutable/reviewed exception-ledger approval workflow. The checkout-local digest remains edit detection only.
- Future operator documentation remains required for the canonical adapters and plan/approval/job/audit/recovery contracts behind deferred App Vault/model/service/storage/local-host mutations and for generic notification-webhook egress controls.
- Reusable checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory -q`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; Python compilation; repository truth; schema/hygiene; backend format/Clippy/full tests; and `git diff --check`.
- Evidence boundary: focused parser tests and the source inventory are `unit-verified`/source-enforcement evidence only. Provider, runtime, browser, Docker, installation, upgrade/recovery, and release qualification remain unperformed.

## M1-04 parser-resolution review-blocked continuation — 2026-09-21

- Developer documentation now records the broader parser contract: exact canonical provenance, ancestor module/use/extern shadow rejection, receiver/reference forwarding, valid contained `#[path]` targets, and escaping path rejection.
- Required next developer documentation: independently immutable exception approval (reviewed baseline/signature or removal of self-updatable evidence), then a final parser contract review. Do not describe the checkout-local exception digest as approval or claim release support.
- Retrospective: learned that full backend tests can exhaust the 512 MiB sandbox `/tmp`; removing only generated `vt-*`/`voidtower-*` fixtures restored space and the exact 693+2 Rust gate passed. Reusable evidence is the 100-test combined Python command, the 144/0 inventory check, source/repository gates, and the Rust commands in `docs/internal/handoffs/2026-09-21-m1-04-parser-resolution-review-blocked-handoff.md`.
- Blocker: no commit was created because the independent review still failed the exception-evidence trust boundary; the latest post-review receiver/cursor fixes have focused tests but await another independent review after the approval boundary is resolved. No runtime/provider/browser/Docker/install/upgrade/recovery/release qualification was performed.

## M1-04 Git-anchored exception approval closure — 2026-09-21

- Developer documentation delivered: `docs/api.md` now documents `--base`, CI's protected pull-request base, local `HEAD` behavior, bounded approval diagnostics, and the two-phase workflow for changing an exception body and regenerating its digest.
- Future developer documentation: define the reviewer/branch-protection ownership for the protected base and document how a legitimate new or changed deferred exception becomes part of an approved base without weakening the source check.
- Future operator documentation: document the canonical plan/approval/job/audit/recovery contracts for deferred App Vault, model, service, storage, and local-host routes, plus generic notification-webhook egress controls; this source gate does not make any of them available.
- Reusable checks: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory scripts.test_repo_truth -q`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check --base HEAD`; Python compilation; schema/hygiene; backend format/Clippy/full tests; and `git diff --check`.
- Evidence boundary: the focused parser/inventory and Git-baseline regressions are `unit-verified`; the production inventory is source-enforcement evidence only. Provider, runtime, browser, Docker, installation, upgrade/recovery, and release qualification remain unperformed.

## M1-04 parser-resolution approval and provenance hardening — 2026-09-21

- Developer documentation updated: `docs/api.md` now states that the active exception registry is Git-base anchored, approval bases must be full commit IDs, and unprotected push contexts are rejected; it also documents explicit unknown receiver provenance for unsupported filesystem value flows.
- Future developer documentation remains required for branch-protection ownership and the reviewed workflow for introducing legitimate new/changed deferred exceptions. Future operator documentation remains required for canonical adapters and their plan/approval/job/audit/recovery contracts.
- Retrospective: independent review reproduced registry-entry, symbolic-base, push-context, and complex receiver gaps; fixes added direct regressions for each. Reusable checks are the combined Python suite, inventory `--check --base <full-commit-id>`, Python compilation, schema/hygiene, Rust fmt/Clippy/full tests, repository truth, and diff checks.
- Evidence boundary: implementation and focused tests are `unit-verified`; the source inventory is source-enforcement evidence only. Provider, runtime, browser, Docker, installation, upgrade, and release qualification remain unperformed until a later supported environment.

## M1-04 final review blocker — 2026-09-21

- Documentation backlog now records a blocked trust-boundary follow-up: the inventory parser has focused fail-closed regressions, but independent review still requires coverage for valid qualified/UFCS, generic, borrowed, function-value, and helper-returned forms and a verifier execution path that cannot be modified by PR-controlled tests.
- Next documentation slice: specify the trusted clean-checkout verifier artifact, its branch-protection ownership, and the canonical compiler-backed mutation extraction contract before documenting broader compatibility support.

## M1-04 trusted verifier bootstrap and syntax contract — 2026-09-21

- Developer/API documentation delivered: `docs/api.md` now names the base-owned `pull_request_target` verifier workflow, its trusted/candidate checkout separation, fail-closed bootstrap behavior, and the explicit bounded mutation-syntax contract.
- Required follow-up documentation: after this commit is merged to a protected branch, record the observed workflow run ID and branch-protection rule that bootstraps `trusted-verifier`; do not claim the gate is active from local YAML tests alone.
- Required future developer documentation remains the compiler-backed Rust module/import/call-shape decision or a maintained expansion of the explicit fail-closed syntax contract. Future operator documentation remains required for canonical adapters and their plan/approval/job/audit/recovery contracts.
- Reusable checks added/updated: `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_compatibility_mutation_inventory.CompatibilityMutationInventoryTests.test_rust_mutation_syntax_contract_is_explicit_and_never_silent -q` and `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_repo_truth.RepoTruthCliTests.test_compatibility_enforcement_uses_trusted_base_verifier_and_untrusted_candidate_as_data -q`. Full final gates remain pending.

## M1-04 trusted verifier bootstrap and syntax contract final — 2026-09-21

- Developer/API documentation is complete for this bounded slice: `docs/api.md` describes the pinned trusted verifier/candidate-data workflow, fork materialization, disabled checkout credentials, fail-closed bootstrap, and explicit unsupported syntax markers.
- Required follow-up documentation: after merge to a protected branch, record the observed `pull_request_target` workflow run and required branch-protection status; local tests do not establish GitHub activation. The next tracked documentation dependency is V6-01 versioned API/event schemas and generated-client drift behavior.
- Retrospective: the final 111-test Python suite, Git-base inventory (`148 classified/0 unknown`), repository truth, schema ownership, hygiene, Rust format, strict Clippy, and full Rust gate (`693 unit + 2 integration + examples`) all passed. Reusable commands are recorded in the dated handoff; no host/provider/browser/Docker/release runtime was available.
- Evidence boundary: `unit-verified` for parser/inventory/workflow contracts and `implemented` for the trusted workflow source boundary; GitHub runtime activation, representative external providers, browser, installation, recovery, and release qualification remain unverified.

## V6-01 activation-gate checkpoint — 2026-09-21

- No end-user API documentation changed because no new V6-01 versioned-schema/generated-client work began in this checkpoint; earlier V6-01 version-negotiation, envelope, and web recovery work remains historical source/test evidence, and the protected-verifier activation prerequisite is unresolved.
- Required developer documentation before V6-01 can proceed: record the first protected `pull_request_target` compatibility-enforcement run ID and the required branch-protection status after the local verifier commits are published to the protected branch.
- Required V6-01 documentation after activation: source-owned resource/action/plan/job/approval/error/inventory/event schema semantics, compatibility/deprecation rules, API-version/error negotiation, bounded SSE gap recovery, generated client/OpenAPI ownership, and drift-check commands. Do not duplicate generated schema fields manually in prose.
- Reusable blocked-checkpoint evidence: `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `git ls-remote origin refs/heads/dev refs/heads/main`; `git diff --check`; and the paginated unauthenticated public GitHub Actions API queries recorded in `docs/internal/handoffs/2026-09-21-v6-01-activation-blocked-handoff.md`.
- Evidence boundary: source/inventory checks passed, but protected GitHub activation and branch protection remain unverified. V6-01 is `blocked`; no generated-contract or runtime qualification claim is made.

## V6-01 activation recheck — 2026-09-21

- No end-user API documentation changed because no V6-01 contract behavior was implemented while protected verifier activation remains unresolved.
- Required developer documentation before V6-01 can proceed: record the first protected `pull_request_target` compatibility-enforcement run database ID and the required status context(s) returned by authenticated branch-protection inspection after publication to the protected development branch.
- Required V6-01 documentation after activation: source-owned resource/action/plan/job/approval/error/inventory/event schema semantics, compatibility/deprecation rules, API-version/error negotiation, bounded SSE gap recovery, generated client/OpenAPI ownership, and exact drift-check commands. Generated fields must not be duplicated manually in prose.
- Reusable blocked-checkpoint commands: `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check`; `python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `git ls-remote origin refs/heads/dev refs/heads/main`; and `git diff --check`.
- Evidence boundary: local source/inventory checks are passed, but protected GitHub activation and branch protection remain unverified. V6-01 remains `blocked`; no generated-contract or runtime qualification claim is made.

## V6-01 activation recheck session — 2026-09-21

- No end-user API documentation changed because no new V6-01 contract behavior was implemented in this session while protected verifier activation remains unresolved; existing version-negotiation, envelope, and SSE-recovery behavior remains historical source/test evidence.
- Required developer documentation before V6-01 can proceed: record the first protected `pull_request_target` compatibility-enforcement run database ID and the required status context(s) returned by authenticated branch-protection inspection after publication to the protected development branch.
- Required V6-01 documentation after activation: source-owned resource/action/plan/job/approval/error/inventory/event semantics, compatibility/deprecation and negotiation/error rules, bounded SSE gap recovery, generated client/OpenAPI ownership, and exact drift-check commands without manually duplicating generated fields.
- Reusable blocked-session commands: `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `git ls-remote origin refs/heads/dev refs/heads/main`; and `git diff --check`.
- Evidence boundary: source/inventory checks passed, but protected GitHub activation and branch protection remain unverified. V6-01 remains `blocked`; no generated-contract or runtime qualification claim is made.

## V6-01 activation recheck session 2 — 2026-09-21

- No end-user API documentation changed because no V6-01 contract behavior was implemented while protected verifier activation remains unresolved.
- Required developer documentation before V6-01 can proceed: record the first protected `pull_request_target` compatibility-enforcement run database ID and the required status context(s) returned by authenticated branch-protection inspection after publication to the protected development branch.
- Required V6-01 documentation after activation: source-owned resource/action/plan/job/approval/error/inventory/event semantics, compatibility/deprecation and negotiation/error rules, bounded SSE gap recovery, generated client/OpenAPI ownership, and exact drift-check commands without manually duplicating generated fields.
- Reusable blocked-session commands: `git ls-remote origin refs/heads/dev refs/heads/main`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `bash scripts/check-repository-hygiene.sh`; `bash scripts/check-schema-migration-ownership.sh`; and `git diff --check`.
- Evidence boundary: local source/inventory/hygiene/schema/diff checks passed, but protected GitHub activation and branch protection remain unverified. V6-01 remains `blocked`; no generated-contract or runtime qualification claim is made.

## V6-01 activation recheck session 3 — 2026-09-21

- No end-user API documentation changed because no V6-01 contract behavior was implemented while protected verifier activation remains unresolved.
- Required developer documentation before V6-01 can proceed: record the first protected `pull_request_target` compatibility-enforcement run database ID and the required status context(s) returned by authenticated branch-protection inspection after publication to the protected development branch.
- Required V6-01 documentation after activation: source-owned resource/action/plan/job/approval/error/inventory/event semantics, compatibility/deprecation and negotiation/error rules, bounded SSE gap recovery, generated client/OpenAPI ownership, and exact drift-check commands without manually duplicating generated fields.
- Reusable blocked-session commands: `git ls-remote origin refs/heads/dev refs/heads/main`; `git ls-tree -r --name-only origin/dev -- .github/workflows/compatibility-enforcement.yml`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `bash scripts/check-repository-hygiene.sh`; `bash scripts/check-schema-migration-ownership.sh`; and `git diff --check`.
- Evidence boundary: local source/inventory/hygiene/schema/diff checks passed, but protected GitHub activation and branch protection remain unverified. V6-01 remains `blocked`; no generated-contract or runtime qualification claim is made.

## V6-01 durable-operation envelope sub-slice — 2026-09-21

- End-user API documentation changed in `docs/api.md`: durable job/approval responses use v1 envelopes across canonical and adopted compatibility submissions; approval comments are capped at 500 characters; event-stream bearer credentials use the `Authorization` header and not query parameters.
- Future documentation required: complete source-owned resource/action/inventory/event schemas, OpenAPI/generated-client ownership, compatibility/deprecation windows, SSE reconnect/gap recovery client behavior, and protected drift enforcement after verifier activation.
- Reusable checks: `cd backend && cargo test --all-features operation_workflows -- --nocapture`; `cd backend && cargo test --all-features api::version::tests -- --nocapture`; `cd backend && cargo test --all-targets --all-features`; `cd backend && cargo clippy --all-targets --all-features -- -D warnings`; `cargo fmt --manifest-path backend/Cargo.toml --check`; `cd frontend && npm test`; `cd frontend && npm run type-check`; `cd frontend && npm run lint`; `cd frontend && npm run build`; `node scripts/generate-api-contract.mjs --check`; `python3 scripts/repo_truth.py --repo . --json --check`; `python3 scripts/compatibility_mutation_inventory.py --repo . --check`; and `git diff --check`.
- Evidence boundary: durable-operation envelope boundary is integration-verified locally; broader V6-01 remains blocked on protected verifier activation and the listed contract breadth. Runtime/provider/browser/release qualification is not claimed.

## V6-01 durable-operation envelope session closure — 2026-09-21

- End-user documentation delivered in `README.md`, `docs/api.md`, `docs/api-tokens.md`, `docs/integrations/odysseus.md`, and the Integrations setup snippet for v1 durable envelopes, bounded approval comments, consistent compatibility responses, and header-only SSE bearer authentication.
- Future documentation remains required for complete resource/action/inventory/event schemas, OpenAPI/generated-client ownership, compatibility/deprecation windows, SSE reconnect/gap recovery client behavior, and protected drift enforcement after verifier activation.
- Reusable final checks are recorded in the dated handoff; the exact verified commit is `83ef1d39b01dc76b05759892a1e9277164f85dd9`.

## V6-01 resource and event read-contract continuation — 2026-09-21

- End-user API documentation changed in `docs/api.md`: resource list/read/capability and event-history response envelopes, pagination bounds, client parser validation, and authoritative history-read semantics are documented.
- Future documentation required: source-owned action/inventory schemas, generated/OpenAPI client ownership, compatibility/deprecation windows, and protected drift enforcement after verifier activation. Runtime/browser/release documentation remains unclaimed.
- Reusable checks: `cd backend && cargo test --all-features api::event_stream_tests -- --nocapture`; `cd backend && cargo test --all-features api::version::tests -- --nocapture`; `cd frontend && npm test -- --run src/api/envelopeClient.test.ts src/api/operationsClient.test.ts src/api/generatedApiContract.test.ts src/operations/durableEvents.test.ts`; `cd frontend && npm run type-check`; `cd frontend && npm run lint`; `cd frontend && npm run build`; `cargo fmt --manifest-path backend/Cargo.toml -- --check`; `node scripts/generate-api-contract.mjs --check`; `python3 scripts/repo_truth.py --repo . --json --check`; and `git diff --check`.
- Evidence boundary: this read-contract continuation is integration-verified locally at the backend real-router and frontend parser/client boundaries; protected activation and runtime/browser/release qualification remain blocked or unclaimed.

## V6-01 action and inventory contract continuation — 2026-09-21

- End-user/operator documentation changed in `docs/api.md`, `docs/agent/inventory-upload.md`, and `docs/agent/linux-agent-service.md`: authenticated inventory upload now documents the versioned nested success envelope, bounded request validation, replay/conflict behavior, agent acknowledgement checks, retry behavior, and the separation between router evidence and service runtime qualification.
- Future documentation required: complete source-owned action/plan schema field generation, OpenAPI ownership, compatibility/deprecation windows, protected drift enforcement after verifier activation, and a supported-host agent runtime runbook.
- Reusable checks: `cd backend && cargo test api::version::tests --all-features`; `cd backend && cargo test api::cmdb::tests::inventory_upload --all-features -- --nocapture`; `cd backend && cargo test agent::transport::tests::inventory_upload --all-features -- --nocapture`; `cd backend && cargo test cmdb::contracts::tests::snapshot_validation --all-features`; `cd frontend && npm test -- --run src/api/envelopeClient.test.ts src/api/generatedApiContract.test.ts`; `cd frontend && npm run type-check`; `cd frontend && npm run lint`; `node scripts/generate-api-contract.mjs --check`; and `git diff --check`.
- Evidence boundary: the action/inventory contract continuation is integration-verified locally at backend real-router/agent transport and frontend parser boundaries after the final change. Protected GitHub activation, agent service runtime, browser, provider, Docker, installation, upgrade/recovery, and release qualification remain unverified.

## V6-01 action-plan schema and OpenAPI continuation — 2026-09-21

- End-user/developer documentation changed in `docs/api.md`: complete `PlanViewV1`, canonical input/result bounds, `400 invalid_action_input`, `422 planning_rejected`, generated OpenAPI ownership, and the explicit runtime-qualification boundary are documented.
- Future documentation required: action-specific schema catalogs and compatibility/deprecation windows once additional action families are source-owned; protected drift-enforcement evidence after verifier activation; and named runtime/browser/release qualification for the canonical web client.
- Reusable checks: `cd backend && cargo test --all-targets --all-features`; `cd backend && cargo clippy --all-targets --all-features -- -D warnings`; `cargo fmt --manifest-path backend/Cargo.toml -- --check`; `cd frontend && npm test && npm run type-check && npm run lint && npm run build`; `node scripts/generate-api-contract.mjs --check`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `bash scripts/check-schema-migration-ownership.sh`; and `git diff --check`.
- Evidence boundary: this continuation is integration-verified at the real backend router/worker and frontend parser/build boundaries. The sandbox's 512 MiB `/tmp` limit can make parallel/full SQLite and Vitest gates fail with `ENOSPC`; remove only disposable `/tmp/vt-p1-*` and `/tmp/voidtower-*` fixtures before a serial rerun. Do not claim runtime or release support from these local checks.

## V6-01 durable result and envelope hardening — 2026-09-22

- End-user/developer documentation changed in `docs/api.md`: job progress must match immutable plan steps, terminal result/error combinations are explicit, approval/event records are exact-key source-owned shapes, and provider adapters must redact exact configured secret values before worker persistence.
- Future documentation required: action-specific result catalogs and examples for each registered action, plus operator guidance for configuring adapter-side secret redaction and reviewing rejected malformed envelopes.
- Reusable checks: `cd backend && cargo test operations::worker::tests::result_sanitization_rejects_unbounded_shape_before_recursive_redaction --all-features`; `cd backend && cargo test operations::worker::tests::successful_results_redact_credential_named_fields_before_persistence --all-features`; `cd frontend && npx vitest run src/api/envelopeClient.test.ts src/api/operationsClient.test.ts --reporter=verbose`; `cd frontend && npm run type-check && npm run lint && npm run build`.
- Evidence boundary: focused/full backend and frontend contract checks are integration-verified locally. Exact provider-secret redaction is adapter-owned because the worker has no secret-manager values at completion; runtime/browser/provider/Docker and protected verifier qualification remain unavailable.

## V6-01 action-plan schema and approval hardening final — 2026-09-22

- End-user/developer documentation is current in `docs/api.md`: immutable plans, registry-backed schema identity, bounded canonical inputs/results, fail-closed worker persistence, approval expiry/staleness, and the runtime qualification boundary are documented.
- Future documentation required: action-specific result catalogs/examples and compatibility/deprecation windows; an operator runbook should describe approval expiry and the single-process controller's serialized decision/expiry behavior once runtime qualification is available.
- Reusable checks: `RUST_TEST_THREADS=2 cargo test --manifest-path backend/Cargo.toml --all-targets --all-features`; `cargo clippy --manifest-path backend/Cargo.toml --all-targets --all-features -- -D warnings`; `cargo fmt --manifest-path backend/Cargo.toml -- --check`; `cd frontend && npm test && npm run type-check && npm run lint && npm run build`; `node scripts/generate-api-contract.mjs --check`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/repo_truth.py --repo . --json --check`; `PYTHONDONTWRITEBYTECODE=1 python3 scripts/compatibility_mutation_inventory.py --repo . --check`; `scripts/check-schema-migration-ownership.sh`; and `git diff --cached --check`.
- Evidence boundary: implementation commit `9e66e4769054811be864fd9e4e104179d57ec7c8` is integration-verified locally; runtime/browser/provider/Docker, protected verifier activation, installation, upgrade/recovery, and release qualification remain unavailable.