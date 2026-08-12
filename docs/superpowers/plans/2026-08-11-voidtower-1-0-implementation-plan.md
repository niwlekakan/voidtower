# VoidTower 1.0 Implementation Plan

**Status:** Approved by the operator; execution in progress

**Date:** 2026-08-11

**Design:** `docs/superpowers/specs/2026-08-11-voidtower-1-0-product-architecture-design.md`

## 1. Execution rules

This plan implements the approved 1.0 design in dependency order. The work packages below are merge-sized outcomes, not promises that broad feature presence equals release readiness.

Every product work package must include, where applicable:

- Session-role and bearer-scope classification.
- Risk class, change-plan, approval, and durable-job behavior.
- CMDB resource and relationship integration.
- Events, audit, timeline, and structured errors.
- AI read/action/event/report contracts with provenance and redaction.
- Web, desktop, mobile, CLI, and API behavior appropriate to the feature.
- Unit, contract, integration, failure, upgrade, and provider evidence.
- Documentation and support-matrix updates.

No unrelated worktree artifacts are to be absorbed implicitly. In particular, the current untracked golden-path files and CMDB handoff must be reviewed and adopted intentionally before they enter a commit.

## 2. Dependency order

```text
R0 product truth
  -> S0 authorization repair
  -> D0 numbered migrations
  -> J0 capability/action/job/approval foundation
  -> V0 recovery and verification
  -> C0 CMDB
  -> N0 agent protocol and local adapter
  -> N1 remote managed cluster
  -> W0 placement and workloads
       -> PX full Proxmox
       -> AU automation/community catalog
       -> PL isolated plugins
  -> HH household experience
  -> M mobile completion
  -> Q release qualification

AI contracts begin in J0 and are required in every later vertical slice.
```

Work may run in parallel only after its dependencies and shared contracts are merged.

## 3. Phase R0 — Product truth and baseline

### R0-01 — Reconcile the audited worktree

**Depends on:** none

**Primary files:**

- `docs/project-reality-audit-2026-08-11.md`
- `ROADMAP.md`
- `README.md`
- `docs/gap-analysis.md`
- `docs/codebase-map.md`
- `.devteam/queue/P1-06-golden-path-integration-ci.md`
- `backend/tests/golden_path.rs`
- `backend/vt_manual_test.sh`

**Work:**

1. Review the untracked golden-path test and manual script as pre-existing work.
2. Validate them against the P1-06 contract without silently rewriting unrelated behavior.
3. Record which audited claims changed since commit `39d149b`.
4. Land or replace the golden-path work through a focused change with its own evidence.
5. Commit the reality audit as the historical basis for the redesign.

**Acceptance:** the worktree contains no ambiguous implementation artifact, and the audit distinguishes verified facts from unexecuted provider/hardware claims.

### R0-02 — Mechanically generated product inventory

**Depends on:** R0-01

**Primary files:**

- New `scripts/generate-product-inventory.*`
- New generated document under `docs/generated/`
- `backend/src/api/mod.rs`
- `backend/src/api/authz_matrix.rs`
- `backend/src/voidwatch/risk_class.rs`
- `backend/src/api/mcp.rs`
- `app-vault/apps/*.yml`
- `frontend/src/App.tsx`
- `mobile/package.json`

**Work:** generate route, auth class, risk class, built-in MCP tool, App Vault entry, client version, and feature-surface counts from source. CI fails when checked-in generated output drifts.

**Acceptance:** README and roadmap counts are sourced from generated output, eliminating the current 52/54/55 catalog and 13/10 MCP contradictions.

### R0-03 — Version and release identity

**Depends on:** R0-02

**Primary files:**

- `backend/Cargo.toml`
- `frontend/package.json`
- `frontend/src-tauri/tauri.conf.json`
- `mobile/package.json`
- `mobile/app.json`
- `.github/workflows/release.yml`

**Work:** define one pre-1.0 version source and propagation/check mechanism. The existing mobile `1.0.0` value is corrected until the release gates pass.

**Acceptance:** CI proves every artifact reports the same release identity.

## 4. Phase S0 — Authorization and safety repair

### S0-01 — Replace route role denylists with positive authorization

**Depends on:** R0-01

**Primary files:**

- `backend/src/api/authz_matrix.rs`
- `backend/src/api/authz_matrix_tests.rs`
- Affected handlers in `backend/src/api/alerts.rs`, `apps.rs`, `audit.rs`, `automation.rs`, `backups.rs`, `containers.rs`, `firewall.rs`, `services.rs`, `status.rs`, `terminal.rs`, and `timeline.rs`
- A new shared session-role authorization module under `backend/src/auth/`

**Work:**

1. Introduce one positive role/capability check rather than repeated string comparisons.
2. Convert every documented `SessionDenylist` route to the intended minimum permission.
3. Correct owner-excluded admin checks.
4. Replace tests that intentionally assert the vulnerability with denial tests for guest, demo, member, viewer, and other insufficient roles.
5. Preserve exhaustive route coverage.

**Acceptance:** low-trust roles cannot access terminal/container shells, arbitrary automation commands, destructive app/storage operations, or other privileged handlers; owner retains intended access.

### S0-02 — Resolve unintended unauthenticated routes

**Depends on:** S0-01

**Primary files:**

- `backend/src/api/capabilities.rs`
- `backend/src/api/diagnostics.rs`
- `backend/src/api/system.rs`
- `backend/src/api/models.rs`
- `backend/src/api/authz_matrix.rs`
- `backend/src/auth/scope_enforce.rs`

**Work:** decide and encode the intended session role, bearer scope, and demo behavior for every route currently reachable without authentication. Public status endpoints remain explicitly public; host capability, diagnostics, and model mutation endpoints do not become public accidentally.

**Acceptance:** all live routes have one explicit session/auth classification, bearer classification, and risk classification, with real-router probes.

### S0-03 — Route/action registry convergence

**Depends on:** S0-02

**Primary files:**

- `backend/src/api/authz_matrix.rs`
- `backend/src/auth/scope_enforce.rs`
- `backend/src/voidwatch/risk_class.rs`
- New shared action metadata module

**Work:** replace three drifting hand-maintained route ledgers with one source of action metadata that can generate or validate session roles, bearer scopes, risk classes, approval requirements, and AI exposure.

**Acceptance:** adding a route or action without complete metadata fails compilation or tests.

### S0-04 — Seeded adversarial action evaluation

**Depends on:** S0-03

**Primary files:**

- New test corpus under `backend/tests/fixtures/security/`
- `backend/src/api/mcp.rs`
- `backend/src/api/integrations.rs`
- `backend/src/api/studio.rs`
- `.github/workflows/ci.yml`

**Work:** exercise every irreversible/denylisted action through MCP, Studio, automation/webhook, and bearer paths using seeded prompt-injection attempts and secret-bearing outputs.

**Acceptance:** the real ingress path denies or queues every protected action, redacts the corpus, and produces an audit record.

## 5. Phase D0 — Numbered schema migrations

### D0-01 — Freeze and verify the current schema

**Depends on:** S0-01

**Primary files:**

- `backend/src/db/mod.rs`
- Existing schema golden fixture and upgrade tests
- New migration directory under `backend/migrations/`

**Work:** regenerate and review the live schema golden file, inventory ignored `ALTER TABLE` paths, and construct representative seeded pre-1.0 databases.

**Acceptance:** fresh and seeded-upgrade schemas are byte-for-byte structurally comparable with explicit exceptions only.

### D0-02 — Convert initialization to numbered migrations

**Depends on:** D0-01

**Primary files:**

- `backend/migrations/0000_baseline.sql`
- Subsequent compatibility migrations
- `backend/src/db/mod.rs`
- `backend/Cargo.toml`

**Work:** move schema ownership from inline table creation and ignored alterations to an ordered migration runner while preserving existing data.

**Acceptance:** fresh install, seeded upgrade, interrupted migration, repeated startup, and backup/restore tests pass.

### D0-03 — Migration policy and tooling

**Depends on:** D0-02

**Work:** document forward-only migration rules, transaction boundaries, destructive-change procedure, fixture generation, and release downgrade expectations. CI rejects direct schema creation outside the migration system.

## 6. Phase J0 — Shared capability, job, approval, and event foundation

### J0-01 — Typed resource and capability registry

**Depends on:** S0-03, D0-02

**Primary files:**

- New modules under `backend/src/resources/` and `backend/src/capabilities/`
- `backend/src/api/mod.rs`
- `backend/src/error.rs`

**Work:** define versioned resource kinds, read capabilities, actions, events, reports, permissions, risks, redaction, and provider requirements. Existing routes initially adapt to this registry without a big-bang rewrite.

**Acceptance:** a capability declaration can generate API/action metadata and an AI-facing schema while using the same authorization decision.

### J0-02 — Durable jobs and operation envelopes

**Depends on:** J0-01

**Primary files:**

- New migrations for jobs, attempts, steps, outputs, and idempotency
- New `backend/src/jobs/`
- `backend/src/main.rs`
- `backend/src/api/events.rs`

**Work:** implement persistent queued/running/succeeded/failed/cancelled/expired states, idempotency keys, preconditions, retries, deadlines, structured progress, verification, and restart recovery.

**Acceptance:** simulated process restarts, duplicate submissions, stale preconditions, cancellation, and expired jobs produce deterministic results.

### J0-03 — Approval queue and mandatory change plans

**Depends on:** J0-02

**Primary files:**

- New approval migrations and backend module
- `backend/src/voidwatch/`
- `frontend/src/components/ui/ChangePlanModal.tsx`
- New web/mobile approval surfaces

**Work:** persist pending approvals with actor, target, risk, plan digest, expiration, one-time/time-limited decisions, and policy-creation follow-up. A plan change invalidates prior approval.

**Acceptance:** irreversible actions cannot execute in any mode without valid human approval; UI, API, AI, automation, and plugins observe the same queue.

### J0-04 — Structured events, audit, and errors

**Depends on:** J0-02

**Primary files:**

- `backend/src/api/events.rs`
- `backend/src/audit/mod.rs`
- `backend/src/api/timeline.rs`
- `backend/src/error.rs`
- `frontend/src/api/client.ts`
- `mobile/src/api/client.ts`

**Work:** define stable event envelopes and error codes with correlation, resource, job, node/provider, retryability, remediation, and redacted diagnostics. Persist durable events required for reconciliation; retain SSE for live delivery.

**Acceptance:** web, mobile, CLI, automation, and AI render the same underlying failure and can trace it to audit evidence.

### J0-05 — Foundation AI contracts

**Depends on:** J0-01, J0-04

**Primary files:**

- `backend/src/api/mcp.rs`
- `backend/src/api/ai_context.rs`
- `backend/src/ai/`
- `backend/src/api/redact.rs`

**Work:** generate resource reads, typed actions, event subscriptions, report schemas, provenance, and redaction from the shared registry. Distinguish node `vt-agent` from UI AI actors in APIs and persistence.

**Acceptance:** every registered resource exposes machine-readable AI parity metadata, and no provider change alters permissions.

### J0-06 — Versioned public API and client generation

**Depends on:** J0-01, J0-04

**Primary files:**

- `backend/src/api/mod.rs`
- New API schema generation module
- `frontend/src/api/`
- `mobile/src/api/`
- New generated SDK packages

**Work:** define compatibility and deprecation policy, publish an OpenAPI document for stable HTTP resources/actions, preserve compatibility adapters for existing `/api/*` clients, and generate TypeScript plus initially supported external SDKs from the authoritative contract.

**Acceptance:** web, mobile, plugins, node tooling, and public clients detect incompatible contract drift in CI instead of discovering it at runtime.

## 7. Phase V0 — Recovery and verification foundation

### V0-01 — Complete golden-path CI

**Depends on:** R0-01, S0-01

**Work:** finish the real Docker/App Vault, container lifecycle, and restic backup/restore-test job described by P1-06. Inspect and either adopt or replace the existing untracked test artifacts.

**Acceptance:** a required CI job runs on pull requests and main and cleans up deterministically.

### V0-02 — Control-plane backup and replacement-host restore

**Depends on:** D0-02, J0-02

**Primary files:**

- `backend/src/api/disaster.rs`
- `docs/recovery.md`
- New recovery manifest and integration tests

**Work:** export and restore the database, configuration, encrypted secrets, CA/trust material, plugin metadata, and required filesystem state with versioned manifests and verification.

**Acceptance:** a replacement control plane restores asset IDs, audit history, credentials, and node trust, then reconciles before mutation.

### V0-03 — Installation and upgrade lanes

**Depends on:** D0-03, V0-01

**Primary files:**

- `scripts/install.sh`
- `packaging/systemd/voidtower.service`
- `docker-compose.yml`
- `.github/workflows/`

**Work:** repair the stale checked-in systemd unit, test clean Docker/systemd installs, upgrades, repair, and uninstall in disposable environments.

**Acceptance:** documented install paths run from packaged artifacts rather than source assumptions.

## 8. Phase C0 — CMDB and discovery

### C0-01 — Core asset schema and ID allocation

**Depends on:** D0-02, J0-01

**Primary files:**

- New CMDB migrations
- New `backend/src/cmdb/`
- New `backend/src/api/assets.rs`
- `backend/src/api/mod.rs`

**Work:** implement immutable UUIDs, configurable human IDs, transactional counters, classes, types, aliases, lifecycle, administrative metadata, and optimistic concurrency.

**Acceptance:** concurrent asset creation cannot duplicate human IDs; aliases resolve permanently; lifecycle is independent of online state.

### C0-02 — Locations, relationships, history, and tags

**Depends on:** C0-01

**Work:** add generic typed relationships with validity history, hierarchical locations, asset history, notes, ownership, and integration with existing tags/audit/timeline.

### C0-03 — Observation, normalization, and correlation

**Depends on:** C0-01, J0-04

**Work:** store provider observations with provenance and age, normalize into typed candidates, apply configurable precedence, correlate stable identifiers, and create a Review First discovery inbox with merge support.

### C0-04 — Asset user experience and AI parity

**Depends on:** C0-02, C0-03, J0-05

**Primary files:**

- New `frontend/src/pages/Assets.tsx` and supporting components
- `frontend/src/api/client.ts`
- New mobile asset screens

**Work:** inventory, detail, discoveries, relationships, history, settings, search, manual household inventory, and AI explain/report/action entry points.

## 9. Phase N0/N1 — Node protocol and managed cluster

### N0-01 — Versioned agent protocol

**Depends on:** J0-02, J0-04, C0-01

**Primary files:**

- New shared protocol module or small sibling crate
- `backend/src/cluster/mod.rs`
- `backend/src/api/node_enroll.rs`

**Work:** define enrollment, identity, capability, inventory, observation, job, progress, cancellation, heartbeat, upgrade, and reconnect messages with compatibility rules.

**Acceptance:** protocol conformance covers duplicate delivery, out-of-order observations, expiry, reconnect, and version mismatch.

### N0-02 — Certificate enrollment and lifecycle

**Depends on:** N0-01, V0-02

**Work:** replace heartbeat bearer enrollment with short-lived join tokens, node certificates, outbound authenticated transport, rotation, revocation, and recovery behavior.

### N0-03 — Local `vt-agent` and host adapters

**Depends on:** N0-01

**Primary files:**

- New agent binary target
- New host adapter interfaces
- Existing `backend/src/storage/`, `containers/`, `services/`, `networking/`, `vms/`, and `terminal/`

**Work:** place local and remote operations behind the same host contract. The combined install talks to its local agent contract without losing current functionality.

### N0-04 — Disk inventory vertical slice

**Depends on:** N0-03, C0-03

**Work:** extend Linux storage observations with WWN, NVMe UUID, serial, model, form factor, bus, capacity, health, and current paths. Correlate into CMDB and link Storage UI/API to asset UUIDs.

**Acceptance:** the approved host-A/remove/host-B disk test passes without duplication.

### N1-01 — Remote node execution and reconciliation

**Depends on:** N0-02, N0-03, J0-03

**Work:** dispatch durable jobs, persist node progress, reconcile after disconnect, reject expired/superseded commands, and expose fresh/stale/converging/conflicted state.

### N1-02 — Node roles, maintenance, and support tiers

**Depends on:** N1-01

**Work:** derive and override node roles from capabilities; implement labels, maintenance state, eligibility, staged agent upgrades, Linux release support, Windows/macOS endpoint agents, and agentless provider nodes.

## 10. Phase W0 — Placement, Docker, and native virtualization

### W0-01 — Explainable placement engine

**Depends on:** N1-02, C0-02

**Work:** implement hard requirements, capacity, architecture, storage, network, device, health, maintenance, affinity, quota, backup, and recovery constraints with ranked explanations.

### W0-02 — Node-aware Docker and App Vault

**Depends on:** W0-01, N1-01

**Primary files:**

- `backend/src/api/apps.rs`
- `backend/src/api/containers.rs`
- `backend/src/containers/mod.rs`
- `frontend/src/pages/AppVault.tsx`

**Work:** plan and deploy Compose projects to selected nodes, classify volumes as local/shared/portable, track drift and health, and prohibit unsafe silent rescheduling.

### W0-03 — Native libvirt VM provisioning

**Depends on:** W0-01, N1-01

**Primary files:**

- `backend/src/vms/mod.rs`
- `backend/src/api/vms.rs`
- `frontend/src/pages/VMs.tsx`

**Work:** add templates/images, storage/network selection, quotas, ownership, creation plans, durable execution, CMDB relationships, console, lifecycle, snapshots, backup, and verified completion.

### W0-04 — Existing operational-domain convergence

**Depends on:** J0-05, C0-02, N1-01

**Primary files:**

- Existing backend domains for services, storage, network, backups, alerts, files, terminal, firewall, WireGuard, proxy, updates, models, and media
- Corresponding web, Tauri, and mobile surfaces

**Work:** adapt every existing operational resource and mutation to canonical asset relationships, typed capabilities, durable jobs where needed, structured events/errors, AI read/action/report parity, and the appropriate client experiences. Preserve working behavior while eliminating local-only identity and action bypasses.

**Acceptance:** the generated product inventory contains no production domain lacking explicit identity, auth, risk, event, audit, AI, and verification metadata.

## 11. Phase PX — Full Proxmox provider

### PX-01 — Provider boundary and test harness

**Depends on:** J0-01, C0-03

**Primary files:**

- Refactor `backend/src/api/proxmox.rs` behind a provider module
- Provider fixtures and a live-test command

**Work:** separate API transport, normalization, capabilities, actions, task polling, errors, and fixtures. Preserve current behavior while establishing contract tests and a real-environment lane.

### PX-02 — Cluster inventory and CMDB correlation

**Depends on:** PX-01, C0-03

**Work:** normalize clusters, nodes, guests, pools, storage, networks, backups, tasks, and optional local-agent data into canonical assets and relationships.

### PX-03 — Unified QEMU/LXC provisioning and lifecycle

**Depends on:** PX-02, W0-01, J0-03

**Work:** creation, clone, templates, cloud-init, ISO/CT images, resource configuration, console, lifecycle, snapshots, rollback, migration, replication, ownership, quotas, and provider-specific advanced settings.

### PX-04 — Storage, backup, restore, and PBS

**Depends on:** PX-02

**Work:** storage configuration/content, ZFS/LVM/directory/Ceph visibility, backup schedules, archive browsing, PBS datastores/namespaces, restore-to-new/existing guest, validation, and CMDB storage relationships.

### PX-05 — Network, firewall, and passthrough

**Depends on:** PX-02, J0-03

**Work:** bridges, bonds, VLANs, SDN, firewall, PCI/GPU/USB/disk passthrough, risk-aware plans, and hardware/provider verification.

### PX-06 — HA, maintenance, upgrades, and cluster reports

**Depends on:** PX-03, PX-04, PX-05

**Work:** quorum/HA visibility, maintenance workflows, rolling-update visibility, task failure analysis, capacity/placement reports, and complete AI parity.

## 12. Phase AU — Automation and community workflows

### AU-01 — Versioned typed workflow engine

**Depends on:** J0-02, J0-03, N1-01

**Work:** schedule/event/webhook/alert/state triggers; typed steps; conditions; branches; dynamic CMDB targets; approvals; timeouts; retries; cancellation; compensation; and versioned run history.

### AU-02 — Existing cron/shell migration

**Depends on:** AU-01

**Work:** represent current cron shell jobs as high-risk workflow steps, preserve schedules/history, require positive authorization, and remove their current bypass potential.

### AU-03 — Community catalog and verified runner

**Depends on:** AU-01, N1-01

**Work:** manifests for immutable source revision, checksum, compatibility, privilege/network/filesystem/secrets declaration, parameters, risk, verification, uninstall, and rollback. Add opt-in catalog sync and Proxmox community workflow support.

### AU-04 — Visual editor and AI workflow authoring

**Depends on:** AU-01, J0-05

**Work:** visual editing over the versioned typed model, AI generation/explanation with schema validation, dry-run, approvals, and diff review.

## 13. Phase PL — Isolated plugins

### PL-01 — Plugin contract and process host

**Depends on:** J0-01, J0-02

**Primary files:**

- `backend/src/api/plugins.rs`
- New plugin host/runtime modules
- `frontend/src/pages/Plugins.tsx`

**Work:** replace static-ZIP completion claims with signed/versioned manifests, isolated child processes, supervised lifecycle, capability-scoped JSON-RPC, timeouts, resource limits, and health.

### PL-02 — Provider/action/UI SDK

**Depends on:** PL-01, J0-05

**Work:** allow plugins to declare resource providers, observations, typed actions, events, reports, and sandboxed UI panels without direct database or unrestricted host access.

### PL-03 — Plugin security and recovery

**Depends on:** PL-02

**Work:** permission review, install/update plans, revocation, crash isolation, backup/restore metadata, audit, secret handles, and malicious-plugin tests.

## 14. Phase HH — Branded household experience

### HH-01 — Naming and identity

**Depends on:** approved architecture; may begin before C0 implementation

**Work:** replace the internal `HomeOS` label through a dedicated brand exercise. Develop distinct naming directions tied to VoidTower, test them with infrastructure and household users, check domain/package/relevant trademark conflicts, and define navigation, onboarding, mobile, documentation, voice, visual, and verbal usage.

**Acceptance:** the user approves a distinctive public name and identity; `HomeOS` does not ship in customer-facing copy.

### HH-02 — Household resource model and privacy

**Depends on:** C0-02, HH-01

**Work:** people, household groups, rooms, locations, devices, ownership, presence, privacy classifications, retention, per-user AI/provider settings, and Authentik/local identity behavior.

### HH-03 — Initial household integrations

**Depends on:** HH-02, AU-01, PL-02

**Work:** select and implement the 1.0 media, Home Assistant, calendar, notification, and family workflows through providers/plugins and the common capability model.

### HH-04 — Household AI and reports

**Depends on:** HH-03, J0-05

**Work:** contextual explanations, household status, voice flows, scheduled reports, privacy boundaries, and policy-controlled actions.

## 15. Phase M — Smartphone and tablet completion

### M-01 — Mobile architecture and contract generation

**Depends on:** J0-04, R0-03

**Primary files:**

- `mobile/src/api/`
- `mobile/src/navigation/`
- `mobile/app.json`
- Mobile CI workflow

**Work:** align versioned API types, multi-instance profiles, secure pairing, platform credential storage, biometric lock, error/event handling, and CI builds/tests.

### M-02 — Five-area navigation and core operations

**Depends on:** M-01, C0-04, J0-03

**Work:** replace placeholder tabs with Home, Activity, Assets, Automations, and AI; add role-aware cluster/household views, CMDB search/scanning, jobs, alerts, and native approvals.

### M-03 — Mobile AI, voice, and notifications

**Depends on:** M-02, J0-05

**Work:** contextual AI, evidence links, voice input, change-plan sheets, minimal-data push payloads, deep links, scheduled reports, and self-hosted notification integrations where practical.

### M-04 — Smartphone endpoint capabilities

**Depends on:** N0-01, M-01

**Work:** enroll the phone as a constrained CMDB endpoint; report permitted identity, OS/app version, battery, storage, connectivity, notification health, and opt-in location/presence.

### M-05 — Offline, accessibility, and distribution

**Depends on:** M-02, M-03

**Work:** stale-state cache, non-queued destructive actions, reconnect, screen-reader/contrast/tap-target validation, phone/tablet layouts, signed iOS/Android artifacts, store-ready packaging, and Android sideloading.

## 16. Phase UX — Web and desktop product convergence

### UX-01 — Shared information architecture and design system

**Depends on:** J0-06, C0-04, HH-01

**Work:** reconcile Tower/Void modes, household branding, Assets, Activity, Automations, AI, cluster/provider navigation, responsive behavior, accessibility, loading/empty/error states, and design tokens without duplicating domain logic.

### UX-02 — Web/Tauri capability parity

**Depends on:** UX-01, W0-04

**Work:** ensure every production capability has an intentional web and desktop presentation, native desktop integration only where it adds value, and the same authorization, job, approval, evidence, and AI behavior as other clients.

### UX-03 — Contextual AI and reporting experience

**Depends on:** UX-01, J0-05

**Work:** add Ask, Explain, Investigate, Plan, Report, and evidence navigation consistently across resource pages; expose scheduled reports and provider/privacy choices without turning AI Studio into a separate management universe.

## 17. Phase Q — 1.0 release qualification

### Q-01 — Support matrix and real-environment certification

**Depends on:** all production-scoped domain work

**Work:** record exact Linux, architecture, Proxmox, PBS, Docker, libvirt, Windows/macOS endpoint, TrueNAS, mobile, browser, GPU, and storage evidence. Unsupported combinations are experimental.

### Q-02 — Security and dependency release review

**Depends on:** S0, PL, AU, M

**Work:** authorization, token/certificate, AI injection, plugin/community content, secret/privacy, mobile, dependency, SBOM, and artifact-signing review.

### Q-03 — Recovery, upgrade, and failure certification

**Depends on:** V0, N1, W0, PX, AU

**Work:** clean install, pre-1.0 upgrade, control-plane replacement, node reconnect, job replay/expiry, partial workflow failure, provider outage, plugin crash, and mobile offline tests.

### Q-04 — Public product truth

**Depends on:** Q-01 through Q-03

**Work:** rewrite README, roadmap, codebase map, release notes, API docs, operator runbooks, and generated inventory around verified claims. Align all versions and publish signed artifacts only after gates pass.

## 18. Immediate execution queue

The first implementation sequence is intentionally narrow:

1. **R0-01:** review and resolve the existing untracked golden-path work and commit the reality audit.
2. **S0-01:** repair the known low-trust role privilege escalations.
3. **S0-02:** close unintended unauthenticated routes.
4. **S0-03:** converge route/action metadata so the defect class cannot recur.
5. **V0-01:** make the real golden paths merge-gating.
6. **D0-01/D0-02:** establish numbered migrations.
7. **J0-01:** begin the shared resource/capability contract.
8. **HH-01:** begin the household naming/identity process in parallel once the user is ready to review naming directions; it does not block security work.

No new remote execution, Proxmox mutation expansion, community-script runner, plugin execution, or AI mutation surface starts before items 1–4 are green.

## 19. Definition of project completion

The project reaches VoidTower 1.0 only when the system-level acceptance criteria in the approved design pass against release artifacts and the public support matrix. Finishing the work-package list without corresponding evidence does not satisfy the release.
