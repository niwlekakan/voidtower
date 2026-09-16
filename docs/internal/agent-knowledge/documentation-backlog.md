
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
