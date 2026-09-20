
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
