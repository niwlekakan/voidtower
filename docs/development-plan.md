# VoidTower evidence-based development plan

Status: tracked execution authority
Approved workflow: `docs/voidtower-development-agent-workflow.md`
Last source review: 2026-09-01 on branch `dev`
Scope: deterministic, web-first VoidTower 1.0

## 1. How to use this plan

This document is the tracked execution plan. It is not proof that a feature works. A contributor selects exactly one dependency-ready slice from the backlog, proves it at its public seam, runs the applicable gates, records the resulting evidence, and leaves one explicit next dependency.

Authority, from strongest to weakest:

1. Current source, schema, tests, build output, produced artifacts, and observed runtime behavior.
2. This tracked plan, including its invariants, dependency graph, and release gates.
3. The newest dated local handoff in `docs/internal/handoffs/` and the approved design or plan it names.
4. `ROADMAP.md` for product intent and broad sequencing.
5. Older handoffs and `.devteam/active/` cards as historical evidence only.

When prose conflicts with executable evidence, correct the prose. When source conflicts with an invariant in this plan, do not silently bless the source: create a bounded convergence slice or mark the affected release gate `blocked`.

At the start and end of every slice:

- record branch, HEAD, upstream ahead/behind state, staged paths, and unstaged paths;
- preserve unrelated changes, including pre-existing staged files;
- run the source-truth report rather than copying mutable totals into documentation;
- name the exact tests and commands run after the final change;
- inspect the resulting diff and account for every path;
- commit only the slice paths, locally, unless publication was explicitly requested;
- write a dated handoff with the evidence label attained and one next dependency.

At this plan's source review, `backend/src/agent/mod.rs` and `backend/src/agent/state.rs` were pre-existing staged changes. They are not part of any documentation or future slice unless a later handoff explicitly adopts them. Do not modify, unstage, reset, or include them in a commit accidentally.

## 2. Evidence model

### 2.1 Maturity labels

Use only these labels. A higher label includes the lower labels for the same bounded behavior, on the named commit and platform.

| Label | Required evidence |
|---|---|
| `implemented` | The behavior exists in current source and its reachable production path can be identified. Compilation or prose alone is not sufficient for a stronger label. |
| `unit-verified` | Focused deterministic tests exercise the behavior and relevant failure cases after the final source change. Mocked collaborators must be identified. |
| `integration-verified` | A real public seam crosses the relevant modules, authentication, persistence, and serialization boundaries. Provider dependencies may be controlled fixtures, but the test must execute product code rather than inspect source text or workflow YAML only. |
| `runtime-verified` | A built process exercised the behavior against the named real service, OS, hardware, or provider. Record platform, dependency versions or images, command, and observed result. |
| `release-qualified` | A produced release artifact passed install, first start, normal operation, backup/upgrade, restart, and documented recovery or rollback on every platform claimed for that artifact. |
| `blocked` | A prerequisite is missing, a required command failed, a failure is unresolved or flaky, or the required platform/provider is unavailable. Record the blocker; do not substitute a lower-quality test while retaining a higher claim. |

A row may state a highest attained label and separately state `blocked` for the next label. For example, “`integration-verified`; runtime gate `blocked` on missing Windows target” is valid. An ignored, filtered, mocked, source-scanning, or workflow-text assertion must never be reported as runtime evidence.

### 2.2 Evidence record

Every completed slice and release candidate records:

```text
commit: <full commit id>
scope: <slice id and public behavior>
label attained: <one maturity label>
platform/dependencies: <OS, architecture, real services or controlled fixtures>
commands: <exact commands run after the final change>
results: <pass/fail and test/artifact identifiers; no guessed totals>
artifacts: <paths and checksums when applicable>
recovery exercised: <scenario and observed result>
known gaps: <remaining lower or higher gates>
next dependency: <one slice id>
```

Test totals are optional and must be parsed from the exact final command if reported. Source-derived route, action, MCP-server, MCP-tool, migration, App Vault, test-file, and package-version inventories belong in the generated source-truth report, not as manually maintained claims in this document.

## 3. Non-negotiable architecture and security invariants

1. `resources.id` is the only canonical asset UUID. Human asset IDs and provider identifiers are scoped aliases. CMDB tables are projections over canonical resources, not a second identity system.
2. Agents submit bounded observations. They do not choose or overwrite linked canonical asset UUIDs.
3. Inventory reconciliation may update observed identity, runtime, health, discovery, and last-seen facts. It must not overwrite administrator-owned names, descriptions, notes, lifecycle, condition, location, ownership, or other explicitly protected fields.
4. Every machine-capable or external mutation must resolve to one typed action and canonical resource, produce an immutable server-derived plan, pass role/scope and policy evaluation, bind any required approval to that exact plan, execute as a durable job, and append audit plus durable event evidence. HTTP compatibility routes, AI, built-in MCP, standalone MCP, webhooks, automations, plugins, schedulers, and CLI are ingress adapters, never alternate execution engines.
5. Unknown route or action metadata, missing capability data, ambiguous resource identity, stale plans, and insufficient role/scope fail closed before provider mutation.
6. Durable events are bounded history and invalidation facts. They are not the materialized state store; clients recover authoritative state through versioned reads.
7. Provider uncertainty or a partial external outcome becomes `needs_attention` and is reconciled. Callers do not blindly replay a possibly successful mutation.
8. Enrollment and managed-node operation remain outbound-first. The agent does not open an inbound management listener, accept controller-chosen local paths, or become a generic remote shell.
9. Secret values, pairing codes, node tokens, provider credentials, raw provider diagnostics, internal SQL, identity material, and unbounded output do not enter logs, errors, fixtures, plans, events, or generated reports.
10. AI providers and cloud services are optional. Core inventory, policy, job execution, audit, recovery, and web administration remain useful and testable with all AI/cloud integrations disabled.
11. Migrations are numbered, immutable after release, owned by the migration system, backed up before legacy upgrade, and validated semantically. Existing migration `0003` is not edited; a genuine schema change gets the next number.
12. No mutable inventory total or generated surface count is hard-coded into tracked authority documents. Derive it from current source.

## 4. Deterministic web-first 1.0 contract

### 4.1 Required 1.0 outcome

VoidTower 1.0 is a self-hosted Linux control plane with a supported web client that can be installed, started, operated, backed up, upgraded, and recovered without an AI provider or external cloud dependency. Its required release path includes:

- authenticated owner/admin/operator web administration with positive role and token-scope enforcement;
- canonical resources and CMDB projection, including outbound Linux node enrollment, collection, authenticated inventory upload, deterministic correlation, review of ambiguity, and protection of administrator-owned fields;
- one canonical mutation path with typed actions, immutable plans, policy, approvals, durable jobs, audit, events, idempotency, cancellation, restart recovery, and explicit `needs_attention` handling;
- encrypted secret references for provider credentials, with no plaintext provider API keys in generic settings;
- household-aware identity, resource grants, consent, privacy boundaries, and safe defaults for any household feature included in the web release;
- a versioned external API/event contract consumed by the supported web client;
- deterministic CI and release gates, a produced Linux artifact, installation/startup/upgrade/recovery evidence, and current operator documentation.

Linux x86_64 is the minimum server release platform. Other Linux architectures are claimed only after their produced artifacts pass the same release gates. The managed Linux agent may be distributed with the server artifact or as a separately versioned artifact, but it must have explicit installation, state, service, upgrade, and rollback behavior.

### 4.2 1.0 non-goals and experimental surfaces

The following do not block web-first 1.0 unless they are explicitly promoted by a later approved revision and pass all applicable gates:

- a release-supported desktop shell;
- Android or iOS store distribution;
- a release-supported Windows controller;
- Windows agent packaging beyond the contract and compile gate in Phase 4;
- executable third-party plugins as a trusted production extension boundary;
- app-specific standalone MCP servers that call third-party apps directly;
- broad household cloud-replacement services, voice satellites, live agent visualization, and creative media pipelines;
- a permanent autonomous multi-agent/Kanban development team;
- offline mirroring of every App Vault image or third-party provider.

Existing desktop, mobile, plugin, app-specific MCP, AI, and household code may remain visible as experimental only when it is clearly labelled, cannot weaken the 1.0 security boundary, and is excluded from release claims. If mobile or desktop artifacts are shipped as 1.0, they stop being non-goals and must pass Phase 6 qualification.

## 5. Current maturity matrix

This matrix is a source review, not a permanent claim. Re-run the source-truth report and relevant gates before selecting a slice.

| Capability | Highest supported label | Current evidence landmarks | Gap to next label / release disposition |
|---|---|---|---|
| Route/action metadata and positive authorization | `integration-verified` | `backend/src/action_registry.rs`; real-router authorization and scope tests under `backend/src/api/` | Re-run deterministically after each route/action change; no manually copied inventory count. |
| Canonical resources and durable operation kernel | `integration-verified` | `backend/src/operations/`; canonical plan/submit API in `backend/src/api/actions.rs`; compatibility bridge in `backend/src/api/operation_adoption.rs` | Runtime evidence exists only for named golden paths, not for every adapter/provider action. Provider-by-provider runtime qualification remains. |
| Docker/App Vault/restic golden path | `implemented`; integration/runtime gate `blocked` | `backend/examples/golden_path.rs`, `backend/tests/golden_path.rs`, `.github/workflows/ci.yml` | `cargo test --test golden_path` passes two static CI-wiring tests, but that target does not execute the real Docker/App Vault/restic harness. Source and workflow assertions do not establish integration or runtime behavior; execute the harness against named dependencies before promotion. |
| CMDB domain and session APIs | `integration-verified` | `backend/src/cmdb/`, `backend/src/api/cmdb/`, numbered migration ownership, real-router tests | No supported web CMDB workflow or collector runtime has been release-qualified. |
| Outbound agent enrollment, protected state, transport, and heartbeat | `integration-verified` | `backend/src/agent/`, early agent branch in `backend/src/main.rs`, newest handoff | Linux runtime/service packaging is not verified. Inventory tasks are intentionally absent. Windows target verification is blocked. |
| Inventory snapshot model and reconciliation service | `unit-verified` | `backend/src/cmdb/contracts.rs`, `backend/src/cmdb/observations.rs` | The authenticated upload-to-reconciliation success path needs a dedicated real-router integration test and collector-produced fixture. |
| Authenticated inventory upload | `implemented` | `backend/src/api/cmdb/inventory.rs`, shared node-token verification in `backend/src/api/node_enroll.rs` | Success, replay, wrong-node, revocation, ambiguity, protected-field, missing-observation, and bounded-body behavior need one end-to-end test seam. |
| Linux collector | `blocked` | Agent schedule reserves an inventory interval; supervision explicitly omits an inventory task | No collector module, bounded command runner, sanitized fixtures, or real Linux runtime evidence exists. |
| Windows collector and target | `blocked` | Windows state ACL code paths exist | Collector contract/fixtures are absent and the recorded Windows Rust target check did not pass. No support claim is allowed. |
| Built-in HTTP MCP reads | `unit-verified` | `backend/src/api/mcp.rs` and Studio delegation tests | Built-in mutation intake is not canonical; bearer/registry behavior must be intentionally designed rather than widened implicitly. |
| Standalone VoidTower MCP server | `implemented` | `odysseus-mcp-servers/voidtower_server.py`, `docs/integrations/mcp-server.md` | Mutation tools call compatibility routes directly, do not require a stable canonical resource/action adapter, and document VM control under a read scope. No release evidence exists. |
| App-specific standalone MCP servers | `implemented` | Source-derived inventory under `odysseus-mcp-servers/` | Direct third-party app credentials and mutations are outside the deterministic 1.0 trust boundary; experimental until separately designed and qualified. |
| AI provider orchestration | `implemented` | `backend/src/ai/`, `backend/src/api/ai_providers.rs` | Provider API-key writes and reads use generic plaintext `settings`; this is blocked on secret-manager convergence. AI remains optional. |
| Encrypted secret manager | `unit-verified` | `backend/src/api/secrets.rs` and consumers such as Proxmox/OIDC | Establish one non-revealing resolver API, migrate AI provider references, prove rotation/last-use/audit semantics, and remove plaintext compatibility storage. |
| Household authorization/privacy | `implemented` for legacy member/app/storage fragments; `blocked` for 1.0 contract | Baseline member tables and APIs | No canonical household identity, resource-grant, consent, purpose, revocation, privacy-export, or cross-user isolation contract exists. |
| Web client | `unit-verified` for tested workflows and production build | `frontend/src/`, Vitest tests, type-check/lint/build scripts | Release qualification needs versioned contract consumption plus running browser-to-built-server tests, failure UX, accessibility, and artifact smoke evidence. |
| Mobile client | `implemented` | `mobile/src/` and Expo package | No test/lint/type-check/build qualification scripts, version negotiation, supported-server matrix, or store/device runtime evidence. Exclude from 1.0 unless qualified. |
| Desktop shell | `implemented` as source/build configuration | Tauri dependencies and desktop workflow | No current install/upgrade/runtime evidence. Exclude from 1.0 unless qualified. |
| Release artifacts | `implemented` as workflow | `.github/workflows/release.yml` | A workflow definition is not an artifact result. Install, first-start, upgrade, recovery, and architecture-specific evidence remain blocked. |
| Repository truth/hygiene | `unit-verified` | Repository-native `scripts/repo_truth.py`; deterministic CLI fixtures; CI entry gate; repository hygiene and schema ownership checks | R0-02 still needs the clean-checkout release-candidate gate runner and evidence manifest. Historical internal files removed from tracking remain available locally under the ignored `docs/internal/` tree and in Git history. |

## 6. Dependency graph

```text
R0 deterministic truth and release baseline
├── M1 canonical mutation convergence
│   ├── S2 encrypted AI-provider secret resolution
│   └── H5 household grants/consent for machine-capable actions
├── C3 platform-neutral collector contract
│   ├── C3 Linux collector + authenticated CMDB reconciliation
│   └── W4 Windows contract/target gate
└── V6 versioned external contracts
    ├── V6 web runtime qualification
    └── V6 mobile/desktop qualification or explicit exclusion

M1 + S2 + C3 + H5 + V6 web + all release gates
└── Q1 web-first 1.0 release candidate
```

Technical work may proceed in parallel only when the branches above do not touch the same high-collision subsystem and use isolated worktrees. Release sequencing remains strict. Phase numbers express product priority; an independent later-phase contract test may be prepared early, but it cannot be declared release-complete before its upstream gates.

## 7. Phase plan

### Phase 0 — Product truth and release determinism

**Exit outcome:** every contributor starts from a deterministic repository report, stale cards cannot select work, local and CI gates agree, and a release claim can be traced to artifacts and runtime evidence.

Required work:

- generate branch/HEAD/ahead-behind, staged/unstaged paths, newest dated handoff, migrations, package versions, test-file inventory, and source-derived route/action/MCP/App Vault inventories without reading credentials;
- make output deterministic, sorted, machine-readable, and non-zero on missing landmarks;
- classify tests by unit, integration, runtime, and artifact qualification rather than treating all passing commands equally;
- resolve repository-hygiene failures without deleting historical evidence blindly;
- define a clean-checkout release-candidate command that runs the applicable backend, frontend, schema, supply-chain, secret, agent, and artifact gates;
- record flakes as failures until reproduced and fixed; isolated reruns do not erase a failing full run.

**Exit gate:** R0-01 and R0-02 are green on a clean checkout and on the preserved development worktree.

### Phase 1 — Canonical mutation intake

**Exit outcome:** every in-scope machine or compatibility ingress is an adapter to the same canonical action/resource service. No provider mutation occurs inside ingress handlers or standalone clients.

Required work:

- convert the standalone VoidTower MCP mutation tools to canonical resource/action plan-and-submit calls with deterministic idempotency and stable job/approval responses;
- replace the documented `vms:read` permission for `vt_control_vm` with a dedicated mutation scope (`vms:control`) and preserve `vms:read` as read-only;
- expose only specifically approved typed actions to MCP/AI; do not globally weaken `AiExposure` or bearer policy;
- converge built-in MCP/Studio AI mutation, webhooks, automations, schedulers, CLI, and remaining compatibility routes through canonical invocation;
- derive an executable exception ledger for intentionally synchronous reads or ephemeral ticket creation; unknown or newly added mutation paths fail tests;
- prove immutable planning, exact approval binding, policy denial, idempotent replay, stale-plan rejection, cancellation, restart recovery, audit, and event emission at each ingress.

**Exit gate:** source enforcement finds no unclassified provider/destructive call site, and at least one real-router integration test per supported ingress proves the same durable job identity and policy result for equivalent intent.

### Phase 2 — Secret-manager convergence

**Exit outcome:** AI providers and other supported integrations reference encrypted secrets by canonical secret ID; generic settings never hold provider credential plaintext.

Required work:

- introduce one bounded internal secret resolver that accepts a secret ID and purpose, decrypts only in memory, updates last-use evidence, and returns redacted errors;
- make AI-provider create/update accept a secret reference or a new secret value that is encrypted immediately; list responses expose metadata only;
- migrate legacy plaintext provider settings transactionally, preserving rollback and deleting plaintext only after encrypted persistence and reference update succeed;
- bind secret authorization to actor/token/resource grants where external callers can select secrets;
- prove rotation, disabled/missing/corrupt secrets, legacy migration, redaction, and optional-AI startup behavior.

**Exit gate:** database and log scans show no provider API-key plaintext; configured providers work through encrypted resolution; all-AI-disabled startup and core web use pass.

### Phase 3 — Linux collector and CMDB reconciliation

**Exit outcome:** a Linux node collects bounded host/physical-disk evidence, uploads it outbound with its node-bound credential, and CMDB reconciliation links or queues review without overwriting administrator state.

Required work:

- define a platform-neutral `InventorySnapshotV1` collector seam independent of DB rows and server-side UUIDs;
- implement Linux host and physical-disk collection with an explicit `lsblk` JSON field list, bounded process time/output, `/sys` fallbacks, redacted diagnostics, and sanitized fixtures;
- exclude loop/RAM devices, partitions as independent physical assets, transient device-mapper entries, and other approved ephemeral classes;
- treat paths, mountpoints, filesystem UUIDs, and runtime names as observations, not stable identity;
- upload on an independent supervised schedule with jittered backoff, cancellation-aware waits, replay-safe snapshot IDs, and no empty “full inventory” snapshot after collector failure;
- reconcile strong identities automatically, preserve ambiguity for review, mark missing observations without deleting state, and protect administrator-owned fields;
- package and runtime-verify the Linux agent service, state permissions, enrollment, heartbeat, collection, upload, restart, outage recovery, and upgrade.

**Exit gate:** sanitized fixture integration tests and a real Linux runtime produce the same contract; a controller outage and restart do not lose state or create duplicate assets.

### Phase 4 — Windows contract gate

**Exit outcome:** the platform-neutral collector contract is proven implementable for Windows without weakening identity, bounds, state protection, or outbound authentication. Windows is not called supported unless runtime/package gates pass.

Required work:

- define Windows fixtures for host and physical disk evidence using stable Windows identifiers and the same snapshot schema;
- isolate OS-specific collection behind the platform-neutral seam;
- add the Windows Rust target/toolchain to deterministic CI and capture complete diagnostics on failure;
- test ACL-protected state, path/symlink/reparse-point behavior, command timeout/output bounds, redaction, duplicate/ambiguous devices, removable media, and missing properties;
- either produce and runtime-qualify a Windows agent artifact or explicitly exclude it from 1.0 support while retaining the compile/contract gate.

**Exit gate:** contract fixtures and target compilation pass. Runtime support additionally requires install/service/enroll/collect/upload/upgrade/recovery evidence on supported Windows versions.

### Phase 5 — Household authorization and privacy

**Exit outcome:** every household-facing read and mutation has a named actor, resource grant, purpose, consent or owner authorization where required, audit trail, revocation behavior, and least-privilege default.

Required work:

- model household membership separately from infrastructure role while reusing canonical users and resources;
- define owner-administered resource grants and scopes for member, guest, device, AI, automation, and API-token actors;
- add explicit consent/purpose and retention rules for high-sensitivity domains such as location, voice, finance, documents, and parental controls;
- default AI/cloud export to off; disclose exactly what leaves the instance and redact secrets/identity material;
- prove cross-user isolation, grant revocation, session/token revocation, consent withdrawal, privacy export/deletion, and audit visibility;
- route every household mutation through the canonical action/job boundary.

**Exit gate:** a real-router matrix covers every actor/grant combination, negative cross-household probes, revocation, and machine ingress. Any household domain without this evidence remains experimental and absent from 1.0 claims.

### Phase 6 — Versioned client contracts, UI, and mobile qualification

**Exit outcome:** supported clients consume an explicit versioned API/event contract, recover authoritative state after disconnects, and are qualified or excluded independently.

Required work:

- publish versioned schemas for resources, actions, plans, jobs, approvals, errors, inventory, and events; include compatibility and deprecation rules;
- generate OpenAPI/client types from source-owned schemas where practical; generated outputs must have drift checks;
- make SSE a bounded invalidation/history channel and prove reconnect/gap recovery through authoritative reads;
- qualify the production web bundle against a built backend for authentication, CMDB, jobs/approvals, offline AI/cloud behavior, failure UX, accessibility, and reconnect/restart;
- add mobile test, lint, type-check, build, server-version negotiation, secure credential storage, and device runtime gates before shipping mobile as 1.0;
- apply equivalent artifact/runtime gates to desktop, or exclude desktop and mobile from the web-first release.

**Exit gate:** web is `release-qualified`; mobile and desktop are either independently `release-qualified` or explicitly excluded from the 1.0 artifact matrix and user-facing support claims.

## 8. First dependency-ordered slices

“Ready” means scope and acceptance are sufficiently defined to implement. Only the earliest slice whose dependencies are green may become active. Do not use stale `.devteam` status to skip this order.

### R0-01 — Deterministic test/hygiene baseline and source-truth report

**Readiness:** `COMPLETED` — repository-native report, deterministic fixture tests, CI entry gate, and green hygiene baseline established by this slice
**Goal/user outcome:** a new contributor can run one credential-safe command and know the exact repository state, source inventory, applicable gates, and current blockers before changing code.

**Likely files/modules:** project skill linked `scripts/repo_truth.py`; `scripts/check-repository-hygiene.sh`; `.github/workflows/ci.yml`; `backend/tests/golden_path.rs`; tracked authority docs only when a generated claim changes.

**Failing public-seam test:** run the source-truth script from a copied/minimal checkout and from the real repository with `--json --check`; first prove it fails on missing landmarks and proves that no credential file/value is read. Add a deterministic fixture test that shuffles filesystem discovery order and expects byte-identical JSON.

**Acceptance criteria:**

- derives, sorts, and emits all inventories required by the approved workflow without hard-coded totals;
- reports staged and unstaged paths separately and preserves pre-existing staged files;
- identifies the newest dated handoff deterministically;
- exits non-zero outside a VoidTower checkout or when required landmarks/counts cannot be derived;
- maps each changed subsystem to exact focused/full gates and evidence labels;
- repository hygiene has a documented green result or a precise pre-existing blocker with ownership;
- final source-truth JSON parses and contains no secret values or credential-file contents.

**Non-goals:** feature implementation, mass documentation rewrites, deleting historical files, changing schema, or “fixing” unrelated staged agent code.

**Recovery:** the report is read-only. Any hygiene correction is path-limited and reversible; move historical evidence to an approved tracked location before removing the old tracked path. Never use `git clean`, `reset`, or broad restore.

**Completion evidence:** `unit-verified` script fixture tests, exact real-repository `--json --check` output saved outside Git, hygiene/schema ownership results, and final `git status` proving unrelated state is untouched.

**Next dependency:** M1-01.

### M1-01 — Canonical standalone-MCP mutation adapter and correct VM scope

**Readiness:** `COMPLETED 2026-09-03` — integration-verified at the real Axum auth/action router boundary with a fake non-mutating VM adapter; the standalone MCP mutation client is unit-verified and its stdio server startup/tool surface is runtime-smoke-verified. No live provider mutation was performed, so this slice is not release-qualified.
**Goal/user outcome:** a standalone MCP caller can request an approved VoidTower mutation and receive the canonical immutable plan/job/approval result; a read-only VM token can never control a VM.

**Likely files/modules:** `odysseus-mcp-servers/voidtower_server.py`; a new focused Python test module under `odysseus-mcp-servers/tests/`; `backend/src/action_registry.rs`; `backend/src/api/actions.rs`; `backend/src/operations/invocation.rs`; `backend/src/api/scope_bypass_tests.rs`; `docs/integrations/mcp-server.md`; `docs/api-tokens.md`.

**Failing public-seam test:** against a real Axum router, invoke one standalone MCP mutation with (a) `vms:read`, (b) `vms:control`, and (c) missing idempotency. First observe that the current tool posts a compatibility action and the documentation authorizes VM mutation with `vms:read`. The new test requires read-only denial before planning/provider calls and a canonical accepted job for the dedicated mutation scope.

**Acceptance criteria:**

- every in-scope standalone VoidTower MCP mutation resolves a canonical `resource_id` and typed action, then calls plan/submit rather than a provider or legacy execution API;
- a caller-generated operation `request_id` produces a deterministic bounded idempotency key: reusing it for the same intent replays the job, reusing it for changed intent conflicts, and a new ID permits a later equivalent operation;
- `vms:read` permits listing only; `vms:control` is required for VM lifecycle mutation and is registered/tested consistently;
- action exposure is explicit per action and fails closed; no global bearer or AI enablement;
- responses expose stable plan/job/approval/error contracts and redact raw provider bodies;
- policy denial, approval-required, stale plan, wrong resource kind, insufficient scope, and `needs_attention` are tested;
- source enforcement fails if a new standalone VoidTower mutation bypasses the adapter.

**Non-goals:** qualifying app-specific MCP servers, adding new provider actions, widening built-in MCP tools wholesale, or changing frontend UX.

**Recovery:** no MCP retry after an ambiguous transport result without querying by idempotency/job. Existing direct routes remain only as compatibility shims during the slice and are removed from the standalone adapter after parity. If scope migration would strand tokens, add an explicit bounded compatibility migration; never treat `vms:read` as temporary write permission.

**Completion evidence:** 31 Python adapter/source-enforcement/runtime-boundary tests pass for exact canonical routes, operation-scoped deterministic idempotency, actor-scoped recovery lookup, strict typed-response validation, stable/redacted success and error responses, submit ambiguity, explicit tool classification, and real MCP stdio-to-HTTP transport. Real-router Rust tests prove `vms:read` denial before planning, `vms:control` acceptance, same-key changed-intent conflict before another adapter call, same-actor idempotency lookup with cross-token isolation, and correlated durable job/event/audit records. Full backend tests and Clippy, repository checks, focused Rustfmt, and diff checks pass. The runtime-boundary test calls `vt_control_vm` across a fake non-mutating HTTP boundary with exactly one plan and one submit; a separate MCP stdio smoke confirms 29 advertised tools with disabled legacy mutations absent. Exact commands and limitations are recorded in the dated M1-01 handoff.

**Next dependency:** M1-02 machine-ingress convergence and S2-01.

### S2-01 — AI-provider encrypted secret resolution

**Readiness:** `READY AFTER R0-01`; release ordering remains after M1-01
**Goal/user outcome:** configuring an AI provider never stores or returns its API key as plaintext, and VoidTower still operates normally with no AI provider.

**Likely files/modules:** `backend/src/api/ai_providers.rs`; `backend/src/ai/orchestrator.rs`; `backend/src/api/secrets.rs` or a new internal secret-service module; next numbered migration only if reference shape cannot be represented safely; focused API/orchestrator tests; AI-provider settings UI after backend contract is green.

**Failing public-seam test:** create an AI provider with a key through the real router, inspect DB/log/API outputs, and first prove the current key is written to generic `settings`. Require an encrypted `secrets.value_enc` record/reference, successful in-memory resolution, metadata-only list output, and no plaintext occurrence.

**Acceptance criteria:**

- provider configuration stores a canonical secret ID, not a settings key containing plaintext;
- one internal resolver decrypts by secret ID and purpose, updates last-use evidence, bounds values, and returns redacted errors;
- create/update/list/health/stream paths use the same resolver contract;
- a transactional one-time migration converts legacy provider plaintext and removes it only after successful encrypted persistence/reference update;
- rotation is observed without restart; missing, forbidden, disabled, and corrupt secrets fail safely;
- DB, logs, errors, API JSON, plans, and events contain no key value;
- startup, login, CMDB reads, and a canonical non-AI operation pass with every AI provider disabled or unreachable.

**Non-goals:** adding providers, cloud failover, prompt/memory features, household AI preferences, or exposing secret reveal to AI/MCP.

**Recovery:** migration is transactional and restart-safe. Keep a protected pre-migration DB backup under existing migration policy. Failure leaves the old reference intact and provider disabled rather than deleting the only credential. Rotation rollback restores the previous encrypted version through the secret service, never generic settings.

**Completion evidence:** real-router DB assertion, orchestrator tests with a controlled local HTTP provider, redaction corpus, migration fresh/legacy/restart tests, full backend/schema gates, and optional-AI runtime smoke.

**Next dependency:** S2-02 legacy plaintext closure; H5 machine-secret grants later.

### C3-01 — Platform-neutral collector contract and Linux fixtures

**Readiness:** `READY AFTER R0-01`; uses the existing outbound-agent and CMDB snapshot foundations
**Goal/user outcome:** a Linux agent can produce a bounded, privacy-safe host/physical-disk snapshot without database knowledge or server-assigned asset UUIDs.

**Likely files/modules:** new modules under `backend/src/agent/collector/`; `backend/src/cmdb/contracts.rs` only for a justified versioned contract change; sanitized fixture files under a focused test-data directory; agent tests; no upload task yet.

**Failing public-seam test:** pass sanitized `lsblk --json --bytes --output <explicit-field-list>` fixture output through the platform-neutral collector and require an exact `InventorySnapshotV1`. Begin with missing command, malformed/non-UTF8, oversized, timeout, and ambiguous-device failures.

**Acceptance criteria:**

- collector output is independent of DB rows, canonical UUIDs, and controller connectivity;
- command field list, timeout, stdout/stderr caps, entity count, string size, and JSON depth are explicit constants;
- `/sys` fallbacks are bounded and cannot escape approved paths;
- loop/RAM/partition/transient mapper classes are excluded while removable physical media is represented intentionally;
- paths, mountpoints, filesystem IDs, and transient names stay in observations, not stable identity evidence;
- identity precedence and HDD/SSD/M.2 classification match server reconciliation rules;
- diagnostics are bounded/redacted and fixtures contain no real serial, hostname, credential, or user path;
- partial collection does not masquerade as a complete full snapshot.

**Non-goals:** inventory upload, Windows implementation, frontend, packaging, service installation, or automatic registration of ambiguous hardware.

**Recovery:** collection is side-effect free. On failure retain the last successful controller state, emit a bounded local diagnostic, and skip upload; never send an empty full snapshot that marks prior entities missing.

**Completion evidence:** deterministic unit/fixture tests for all listed failures, focused collector tests, full backend gates, schema ownership check, and fixture privacy scan.

**Next dependency:** C3-02.

### C3-02 — Authenticated inventory upload and CMDB reconciliation

**Readiness:** `READY AFTER C3-01`; the endpoint and observation service are implemented but not integration-qualified as one path
**Goal/user outcome:** an enrolled Linux node uploads its own snapshot and VoidTower links, registers, or queues review deterministically without losing administrator edits.

**Likely files/modules:** `backend/src/agent/supervision.rs`; `backend/src/agent/transport.rs`; `backend/src/api/cmdb/inventory.rs`; `backend/src/api/node_enroll.rs`; `backend/src/cmdb/observations.rs`; `backend/src/cmdb/correlation.rs`; CMDB/API integration tests; no frontend.

**Failing public-seam test:** enroll a node through the real router, upload a C3-01 fixture with its node token, then read canonical CMDB state. First require failures for wrong path-node token, unapproved/non-agent node, oversized body, replay conflict, and an observation attempting to overwrite admin-owned fields.

**Acceptance criteria:**

- request authentication binds token, approved agent-capable node, and path node ID before parsing or mutation;
- the node resolves its pre-existing canonical host resource; the payload cannot select any linked `resources.id`;
- same snapshot ID and same intent replay safely; same ID with different intent conflicts;
- strong evidence links deterministically, weak/ambiguous evidence creates review, and missing observations do not delete assets;
- admin name/description/notes/lifecycle/condition/location/ownership remain byte-for-byte unchanged across reconciliation;
- observation, inventory result, audit, and event records share correlation evidence while events remain non-authoritative;
- upload supervision uses independent schedule/backoff, survives controller outage/restart, and never overlaps unboundedly;
- all response/error/diagnostic payloads are bounded and redacted.

**Non-goals:** remote command execution, Windows collection, CMDB frontend, arbitrary resource adoption, or provider mutation.

**Recovery:** snapshot persistence and reconciliation are transactional or restart-resumable. A failed upload is retried by snapshot identity within bounded backoff; an ambiguous HTTP outcome is resolved by replay/query, not a new snapshot ID. Reconciliation failure does not mark previous entities missing.

**Completion evidence:** real-router enrollment/upload/read integration test using C3-01 fixtures, replay/revocation/protected-field tests, full backend/schema gates, then a runtime Linux agent→controller smoke with controller outage and restart.

**Next dependency:** C3-03 Linux agent service qualification and W4-01.

## 9. Subsequent bounded backlog

| Slice | Dependencies | Bounded outcome and acceptance | Explicit non-goal / recovery | Required evidence | Next dependency |
|---|---|---|---|---|---|
| R0-02 Release-candidate gate runner | R0-01 | One clean-checkout command selects and runs applicable gates, produces a machine-readable manifest, fails on skipped mandatory gates, and records artifact hashes. | Does not publish. Interrupted runs retain logs and never mark green. | Integration-verified local/CI parity. | Q1-01 after all product gates. |
| M1-02 Built-in MCP and AI mutation intake | M1-01 | Approved machine actions use canonical resources/actions/jobs; unknown tools/actions fail closed; exact actor/ingress/policy/approval/audit/event links are tested. | No broad tool expansion. Disable mutation exposure on uncertain recovery. | Real-router integration for built-in MCP and Studio/AI. | M1-03. |
| M1-03 Webhook, automation, scheduler, and CLI convergence | M1-02 | Equivalent intent from each supported ingress yields the same canonical plan/job semantics and scoped actor identity. | Shell automation remains experimental if it cannot satisfy immutable planning. Ambiguous outcomes reconcile. | One integration test per ingress plus restart recovery. | M1-04 and H5-01. |
| M1-04 Compatibility bypass closure | M1-03 | Source-derived mutation inventory has no provider call outside adapters; synchronous exceptions are typed, read-only/ephemeral, and executable. | No hand-maintained totals. Unknown call sites block. | Source enforcement plus representative runtime providers. | V6-01. |
| S2-02 Legacy secret closure and rotation | S2-01 | No supported consumer reads secret plaintext from settings; rotation/version/last-use and scoped access are enforced. | No secret values in export. Failed rotation retains last good version. | DB/log scan, integration tests, recovery test. | H5-02 and V6-02. |
| C3-03 Linux agent supervision and service package | C3-02 | Independent heartbeat/inventory loops, bounded backoff, graceful shutdown, service install, protected state, upgrade and rollback. | No inbound listener or generic command channel. Roll back binary while retaining compatible state. | Runtime-verified on supported Linux; artifact evidence. | V6-02 and Q1-01. |
| W4-01 Windows collector contract/fixtures | C3-01 | Same snapshot semantics and privacy/bounds using Windows-native evidence; deterministic sanitized fixtures. | No support claim. Failed collection skips upload. | Unit-verified fixtures. | W4-02. |
| W4-02 Windows target gate | W4-01 | CI target build/check succeeds with captured diagnostics and ACL/path tests. | Packaging remains optional. Compile failure is `blocked`, never waived. | Integration-verified target gate. | W4-03 or explicit 1.0 exclusion. |
| W4-03 Windows runtime package (optional 1.0) | W4-02, C3-02 | Install/service/enroll/collect/upload/upgrade/recovery on named Windows versions. | If not green, exclude Windows agent from 1.0. | Release-qualified artifact. | Q1-01 only if Windows is claimed. |
| H5-01 Household actor and resource grants | M1-03, canonical resource contract | Canonical user/household membership and resource grants; least-privilege matrix; revocation invalidates sessions/tokens/jobs safely. | No new parallel identity provider. Revocation halts new work and reconciles in-flight work. | Schema, real-router auth matrix, migration gates. | H5-02 and V6-01. |
| H5-02 Consent, purpose, retention, and privacy operations | H5-01, S2-02 | Sensitive-domain consent and purpose records, data minimization, export/deletion, AI/cloud off by default. | Does not add broad household services. Failed deletion reports retained legal/technical state explicitly. | Integration tests plus privacy/security review. | H5-03. |
| H5-03 Qualified household web slice | H5-02, V6-01 | One bounded household workflow demonstrates grants, consent, canonical mutation, audit, revocation, and safe UI. | Other domains stay experimental. | Browser-to-server runtime evidence. | V6-02. |
| V6-01 Versioned API/event schemas | R0-01, M1-04, C3-02 | Source-owned versioned schemas, OpenAPI where applicable, generated client types, error/version negotiation, drift CI. | No manually maintained duplicate schema. Old compatible version remains during deprecation window. | Contract and generated-drift tests. | V6-02; optionally V6-03/V6-04. |
| V6-02 Web client qualification | V6-01, S2-02, C3-03, H5-01 | Production web bundle passes auth, CMDB, jobs/approvals, reconnect, AI-offline, error, accessibility, restart, and upgrade paths. | Does not qualify desktop/mobile. Client falls back to authoritative reads after event gaps. | Release-qualified web/server artifact. | Q1-01. |
| V6-03 Mobile contract/build qualification | V6-01, H5-01 | Add tests/lint/type-check/build, secure auth storage, version negotiation, device runtime, reconnect/revocation. | If any gate is absent, mobile is excluded from 1.0. | Runtime/release evidence per OS. | Q1-01 only if mobile is claimed. |
| V6-04 Desktop qualification | V6-01, V6-02 | Produced installers pass clean install, first start, connectivity, upgrade, rollback, and uninstall on claimed OSes. | Exclude rather than infer support from a build workflow. | Release-qualified installers. | Q1-01 only if desktop is claimed. |
| Q1-01 Web-first release candidate | All required phase exits | Build signed/checksummed artifacts from a clean tag; run all gates; install, start, operate, back up, upgrade, recover, and document known limitations. | No publication on partial or flaky evidence. Preserve previous artifact for rollback. | Release-qualified Linux artifact and evidence manifest. | Publication decision. |

## 10. Release gates

A 1.0 tag is blocked until every required gate is green on the exact candidate commit and produced artifacts.

### G0 — Repository truth and reproducibility

- source-truth report succeeds and is archived with the candidate evidence;
- worktree is clean except explicitly recorded generated outputs;
- repository hygiene, schema ownership, formatting/diff checks, and secret scanning pass;
- dependency lockfiles are honored; CI and local gate definitions agree;
- no required result depends on a stale card or copied total.

### G1 — Authentication, authorization, and privacy

- positive session-role and bearer-scope matrices pass at real routers;
- unknown routes/actions/scopes and revoked credentials fail closed;
- node tokens are path/node bound and outbound-first;
- household grants/consent pass negative cross-user tests for included domains;
- logs, APIs, generated docs, fixtures, artifacts, audit, plans, and events pass redaction scans.

### G2 — Canonical mutation and recovery

For every release-supported mutation ingress:

- typed action and canonical `resources.id` resolution;
- immutable plan and stale-state fingerprint;
- policy result and exact approval binding;
- scoped actor and ingress identity;
- durable job/idempotency/cancellation;
- restart/lease recovery and `needs_attention` reconciliation;
- bounded provider output;
- linked audit and durable event evidence;
- executable proof that ingress handlers do not call providers directly.

### G3 — Persistence and migration

- fresh database, supported legacy upgrade, incompatible schema rejection, concurrent startup, backup-before-migration, checksum ownership, and semantic integrity pass;
- CMDB remains a projection over `resources.id` and retained aliases resolve;
- events can be pruned/replayed within policy without losing materialized state;
- downgrade/restore procedure is documented and exercised.

### G4 — Agent and inventory

- agent state/path/token/CA bounds and permissions pass;
- Linux fixture and real-runtime collection agree;
- authenticated upload, replay, outage, restart, ambiguity, missing observation, and protected admin fields pass;
- no empty failed collection can mark inventory missing;
- Windows contract/target has an explicit green or excluded disposition.

### G5 — Secret and optional-integration behavior

- supported credentials are encrypted and referenced by ID;
- rotation, revocation, missing/corrupt secret, and redaction pass;
- database/log scans find no provider plaintext;
- all AI providers and cloud integrations disabled/unreachable does not break core 1.0 workflows.

### G6 — Client contracts and web runtime

- version negotiation, schema compatibility, generated-client drift, stable errors, and SSE gap recovery pass;
- frontend tests, type-check, lint, production build, and browser-to-built-server runtime paths pass;
- auth expiry, denied actions, approval, job progress, `needs_attention`, server restart, and offline AI have usable failure UX;
- mobile/desktop are release-qualified separately or absent from the 1.0 artifact/support matrix.

### G7 — Supply chain and operational failure

- dependency audit/policy checks and secret/history scans pass with every exception explicitly owned and time-bounded;
- disk full, read-only paths, unavailable provider, timeout, truncated output, process crash, controller restart, and network partition produce bounded recoverable behavior;
- backups and restore are exercised against candidate data, not only configuration syntax.

### G8 — Artifact, install, upgrade, and rollback

For each claimed platform/architecture:

- artifact is produced from the candidate tag with checksum and version metadata;
- clean install and first start succeed with documented prerequisites;
- normal core workflow passes against the packaged frontend/backend;
- upgrade from the oldest supported version preserves data and runs migrations safely;
- restart and host reboot preserve service/state;
- rollback or backup restore is exercised and documented;
- uninstall/data-retention behavior is explicit;
- evidence manifest links artifact checksum to commands and observed results.

## 11. Documentation and generation requirements

1. **Source truth:** use the project skill's `scripts/repo_truth.py --json --check` (or the repository-native successor established by R0-01). It must derive current inventories, sort output, stay within the repository root, avoid `.env`/credential contents, and distinguish source presence from runtime support.
2. **No copied totals:** do not write fixed route, action, built-in tool, standalone server, App Vault manifest, migration, integration-test, or package-version totals in authority docs. If a snapshot is useful, embed a generated artifact link with commit ID and command, not a hand-updated number.
3. **API contracts:** Phase V6-01 owns versioned schemas and OpenAPI/client generation. Generated files must include a generator version/source hash and CI drift check. Handwritten prose explains semantics; it does not duplicate schema fields as a second authority.
4. **Action and route docs:** generate tables from typed registry metadata. Documentation generation fails on unknown credential, role, scope, risk, approval, AI exposure, execution, recovery, or ingress metadata.
5. **MCP docs:** derive available tools and required scopes from executable declarations/adapter mappings. Mutation documentation must distinguish plan from submit and name the durable job/approval semantics. App-specific direct servers stay in an experimental section.
6. **CMDB/inventory docs:** generate schema examples from sanitized fixtures and versioned Rust contracts. Examples never contain real serials, hostnames, user paths, tokens, or cloud IDs.
7. **Release evidence:** generate an evidence manifest outside normal source generation containing candidate commit, artifact checksums, platform matrix, exact commands, results, recovery exercises, exclusions, and known gaps. Publication consumes this manifest; it does not infer success from workflow configuration.
8. **Operator docs:** before release, update install, first-run owner setup, agent enrollment, secret setup/rotation, backup/restore, upgrade/rollback, AI-offline operation, troubleshooting, and support matrix. Every command is smoke-tested against the candidate artifact.
9. **Historical documents:** keep old handoffs/cards as evidence, clearly non-authoritative. A generated/current status index may point to them; never reactivate a card merely because it is under an `active` directory.
10. **Handoffs:** each completed slice records the evidence template from section 2.2, exact changed paths, explicit non-goals, unresolved failures, preserved unrelated worktree state, and one next dependency.

## 12. Slice completion checklist

A slice is complete only when all applicable items are true:

- [ ] exactly one slice ID was active;
- [ ] dependencies were green or an explicit blocker was the slice outcome;
- [ ] a public-seam test failed for the intended reason before implementation;
- [ ] the smallest implementation passed focused tests;
- [ ] invariants and explicit non-goals were preserved;
- [ ] focused and full applicable gates ran after the final change;
- [ ] failures and skipped prerequisites are recorded honestly;
- [ ] independent spec and security/code review has no unresolved high or medium finding;
- [ ] recovery behavior was tested at the maturity level claimed;
- [ ] generated documentation/evidence has no stale totals or secrets;
- [ ] every changed path is accounted for and unrelated staged/unstaged work remains untouched;
- [ ] the local commit contains only slice files;
- [ ] the handoff names the attained maturity label and one next dependency.

The first slice to select from a fresh checkout is **R0-01**. After it is green, select **M1-01**. Do not begin from `.devteam/active/` without an explicit tracked-plan and newest-handoff reactivation.
