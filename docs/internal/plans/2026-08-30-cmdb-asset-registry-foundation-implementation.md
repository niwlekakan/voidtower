# CMDB / Asset Registry Vertical Foundation Implementation Plan

Design: `docs/internal/specs/2026-08-30-cmdb-asset-registry-foundation-design.md`
Scope: Resource-backed CMDB, Linux and Windows inventory agents, physical-disk correlation, and
initial Tower/Void inventory integration

## 1. Freeze migration and public domain contracts

- Add `backend/migrations/0003_cmdb_asset_registry.sql` with CMDB profiles, classes, types,
  identifier settings/counters, stable identities, observations/snapshots, relationship types and
  relationships, locations, and the indexes/constraints required by the approved design.
- Key every asset-facing foreign key to `resources.id`; do not add a second asset UUID.
- Use `resource_aliases(namespace='cmdb.asset_id', scope_key='global')` for current and retained
  human-ID resolution rather than adding another alias table.
- Keep provider JSON bounded in application code and constrain relational state/boolean fields in
  SQL where schema checks improve integrity without preventing future status administration.
- Add `backend/src/cmdb/contracts.rs` for versioned asset, settings, discovery, observation,
  relationship, location, snapshot, collector, and API response shapes. Keep agent-upload shapes
  independent from database row structs.
- Add `backend/src/cmdb/mod.rs` and expose the module from `main.rs` without coupling it to API
  handlers or platform commands.
- Extend `backend/src/db/schema.rs` with the third canonical migration, and update
  `backend/tests/schema_golden.sql` plus fresh/upgrade migration expectations.
- Add schema tests proving every CMDB foreign key targets the resource UUID, current asset IDs are
  unique, strong identity/index rules are present, relationship endpoints are restrictive, and an
  existing v0.9/J0 database upgrades without changing its current rows.
- Run focused DB tests, the golden-schema test, and `git diff --check`.
- Commit as the schema/domain-contract checkpoint.

## 2. Implement data-driven catalogs and transactional identifiers

- Add `backend/src/cmdb/catalog.rs` for idempotent built-in class, type, relationship-type, and
  default-setting seeds. Call it from `db::seeds::run` in the existing initialization transaction.
- Seed all approved class keys, storage types, and relationship types with stable display labels.
- Validate custom keys with one bounded URL/identifier-safe grammar and reject disabling/deleting
  catalog rows referenced by active assets or relationships.
- Add `backend/src/cmdb/identifiers.rs` for settings parsing, template validation, rendering,
  normalization/case rules, counter-scope calculation, and allocation inside a caller-owned SQLite
  transaction.
- Acquire identifier counters through a write-serialized transaction and insert the asset/current
  alias before commit. Treat uniqueness races as retryable allocation conflicts within a strict
  attempt bound; never issue an ID outside the asset transaction.
- Preserve `0000`, configurable start/width/separator/template, and the default independent
  class-plus-type counters.
- Add unit tests for default sequences, independent types, custom templates, case behavior,
  reserved/overflow values, invalid/oversized output, configuration changes, and concurrent
  allocation through a multi-connection pool.
- Run focused catalog/identifier tests and strict Clippy on the new modules.

## 3. Build the CMDB asset service over canonical resources

- Add `backend/src/cmdb/assets.rs` with caller-owned transaction helpers and public service methods
  for manual create, list/search, selector resolution, detail, patch with expected revision, rename,
  aliases, retire, restore, tags, and history.
- Manual creation inserts `resources`, `cmdb_assets`, the current CMDB alias, and
  `cmdb.asset.created.v1` atomically. Do not call the observation-oriented `resources::observe`
  path for an administrator-owned manual asset.
- Resolve selectors in the exact order UUID, current human ID, then retained CMDB alias. Ambiguous
  or conflicting selectors fail closed; provider aliases are never treated as public asset IDs.
- On rename, retain the old alias row, add the new current alias, update the profile/revision, and
  append a bounded `cmdb.asset.identifier_changed.v1` event in one transaction.
- Permit removal only for retained optional aliases; reject deletion of the alias equal to the
  current `cmdb_assets.asset_id` so every asset remains resolvable by its displayed identifier.
- Map rich CMDB lifecycle into coarse resource lifecycle explicitly so retired/disposed/lost assets
  cannot receive new jobs while discovery state alone never changes administrator lifecycle.
- Build list/search with `QueryBuilder`, bound `limit`/`offset`, explicit filters, escaped LIKE
  patterns, and a bounded shorthand parser for documented fields.
- Add a transaction-aware audit insert helper under `backend/src/audit` or CMDB service code so
  CMDB state, durable events, and audit records commit/rollback together. Preserve the existing
  best-effort audit API for unrelated callers.
- Add tests for manual assets without providers, selectors/aliases, optimistic revision conflict,
  retirement/restore, lifecycle mapping, search/filter/tag/location joins, redaction/limits, and
  rollback invisibility in both durable events and audit.

## 4. Implement locations and generic relationships

- Add `backend/src/cmdb/relationships.rs` and `backend/src/cmdb/locations.rs`.
- Validate relationship type availability, source/destination existence, non-self edges where the
  type requires it, attachment metadata bounds, and active-edge uniqueness.
- Starting a physical `installed_in` edge ends every other active `installed_in` edge for that
  asset in the same transaction before creating the new edge.
- Ending or moving a relationship appends correlated per-asset durable events so history queries on
  either endpoint remain indexed and complete.
- Validate location parents transactionally and reject self-parenting, descendant cycles, deletion
  while referenced, and paths beyond a fixed depth.
- Add focused tests for duplicate edges, movement races, both-sided history, ended-edge
  preservation, metadata limits, location cycles, and referential deletion failures.

## 5. Add observation persistence, correlation, and discoveries

- Add `backend/src/cmdb/observations.rs` for snapshot ingestion, normalized identity evidence,
  observation upsert, fingerprinting, missing-state convergence, and inbox decisions.
- Add `backend/src/cmdb/correlation.rs` as the only physical-identity matcher. Remote agents,
  Proxmox evidence, and local Storage call this service instead of implementing their own matching.
- Normalize and validate WWN, NVMe persistent identifiers, model/serial composites, serial-only
  evidence, provider namespaces, capacity, rotation, interface, protocol, and form-factor fields.
- Match exact validated WWN, then validated NVMe identity, then unique serial-plus-model. Treat
  serial-only, malformed, multiply matching, or contradictory evidence as review state.
- For trusted enrolled nodes, auto-create unmatched strong identities with the correct physical
  storage class/type, stable identity rows, observation, current host relationship, events, and
  audit in one transaction.
- Store a node/snapshot idempotency record before reconciliation and return the prior result for a
  completed replay. A rollback leaves the snapshot retryable.
- Only a completed full snapshot marks omitted observations missing. Upload or heartbeat failures
  do not. A new strong match on another host immediately ends the previous active attachment.
- Implement discovery register, edit-and-register, link existing, and ignore with fingerprint-based
  suppression. A material evidence change may reopen an ignored discovery.
- Add tests for path-independent identity, replay, rollback/retry, missing-without-delete,
  Linux-to-Windows movement, simultaneous host evidence, conflicts, weak review, ignore/reopen,
  M.2 SATA versus NVMe, and no discovery overwrite of administrator fields.

## 6. Mount the canonical CMDB API and authorization boundary

- Create a focused `backend/src/api/cmdb/` module tree for assets, catalogs, locations,
  relationships, discoveries, and settings rather than one oversized handler file.
- Mount every approved `/api/cmdb/*` route and `POST /api/nodes/:id/inventory` in
  `backend/src/api/mod.rs`.
- Use the shared positive role guards: owner/admin for CMDB mutations and settings; owner/admin/
  operator for reads; lower and unknown roles fail closed.
- Refactor node-token verification in `api/node_enroll.rs` into one constant-time helper shared by
  heartbeat and inventory. Bind the credential to the exact path node ID.
- Extend enrollment with an explicit `provision_wireguard` field whose compatibility default
  preserves existing clients. The new agent command sends false unless requested. When false,
  enrollment must not require WireGuard availability.
- Add structured request limits, collection-count limits, stale-revision conflicts, and stable
  error envelopes before provider/correlation work.
- Add complete route metadata in `backend/src/action_registry.rs`. CMDB writes remain direct
  inventory CRUD, not provider mutations or canonical action jobs; physical operations continue to
  use the existing typed action boundary.
- Add a real-router CMDB test module covering exact role allowlists, unauthenticated access, node
  token ownership/revocation, payload limits, manual CRUD, alias selectors, discovery decisions,
  settings, locations, relationships, and structured failures.
- Extend authz/source-inventory tests so every new route is registered exactly once and node tokens
  cannot reach generic bearer/session/action paths.

## 7. Turn the existing binary into an outbound inventory agent

- Add `backend/src/agent/` modules for persistent configuration, client transport, collector
  contract, Linux collector, Windows collector, and runtime supervision.
- Add `voidtower agent enroll` under the existing Clap command tree. Accept server URL, pairing
  code, display name, device type, optional CA path, optional WireGuard provisioning, and explicit
  state path overrides suitable for packages/tests.
- Persist server URL, node UUID, token, optional CA, and schedule settings atomically. On Unix use
  owner-only mode; on Windows restrict the file ACL to the service/current account. Never log or
  serialize the token into diagnostics.
- Make `--agent` load agent state and start only heartbeat/inventory tasks before controller DB,
  secrets, frontend, workers, schedulers, or listener initialization. Retain the flag for backward
  compatibility with the existing CLI promise.
- Use one configured `reqwest::Client`, normal certificate validation, optional CA, bounded
  request/connect timeouts, exponential backoff with jitter, and a cancellation-aware schedule.
- Send heartbeats independently of inventory. A collection failure reports bounded health and does
  not send a misleading empty full snapshot.
- Add tests with a local in-process router proving enrollment persistence, outbound-only behavior,
  TLS/URL validation, retry/backoff bounds, clean shutdown, no controller initialization, and token
  secrecy.

## 8. Implement Linux and Windows host/disk collectors

- Define a platform-neutral collector output containing host evidence and physical-disk entities;
  keep provider-native raw fields out of the canonical contract unless explicitly bounded.
- Linux invokes `lsblk` with an explicit constant JSON field list for bytes, topology, model,
  vendor, serial, WWN, transport, rotation, filesystem/mount state, and path. Read `/sys` only for
  known bounded fallback properties.
- Exclude loop/RAM devices, partitions as standalone physical assets, device-mapper temporaries,
  and other approved ephemeral classes before upload.
- Windows invokes one constant encoded/script resource using built-in PowerShell/CIM cmdlets and
  parses bounded JSON for machine UUID, physical disks, disks, and associations. Never concatenate
  user input into PowerShell.
- Treat Linux device paths, Windows disk numbers, drive letters, and volume IDs as runtime
  observations. Emit stable identity evidence separately.
- Add captured Linux and Windows fixture files under `backend/tests/fixtures/cmdb/` with no real
  serials, hostnames, credentials, or user paths.
- Test missing optional commands/properties, non-UTF8/bad JSON, timeouts, duplicate/ambiguous
  devices, removable media, HDD/SSD/M.2 classification, and diagnostic truncation/redaction.
- Compile/test platform-gated code for the current host and add CI `cargo check` targets for
  supported Linux and Windows Rust targets without requiring platform commands during the build.

## 9. Adopt existing resources without duplicating identity

- Add `backend/src/cmdb/adoption.rs` and call it from post-migration seeds after catalog setup.
- Project the seeded local system and existing `voidtower.node`/legacy-node resources as
  `sys/host`, preserving their resource UUID and aliases.
- Project current Docker container resources as service/application assets, Proxmox QEMU/LXC
  resources as VM/container assets, and Proxmox storage resources as data/storage-pool assets.
- Allocate missing human IDs in deterministic resource creation/UUID order inside bounded
  transactions; make repeated startup a no-op.
- Do not project firewall rules, update targets, approval/job records, or other operational-only
  resources.
- Extend Proxmox disk observation only where its provider response contains strong identities.
  Retain path-only evidence as scoped observations and never auto-create a physical asset from a
  Proxmox device path.
- Call the same idempotent projection helper after future Docker container and Proxmox guest/storage
  observations, not only during startup backfill, so resources discovered after boot receive the
  same CMDB profile. A failed projection is reported and safely repaired by the next inventory pass;
  it never changes or deletes the canonical operational resource.
- Add backfill/adoption tests against representative pre-CMDB resources, including repeated seed
  runs, retained aliases/capabilities/jobs, deterministic counters, and no extra durable events on
  no-op startup.

## 10. Converge local Storage on the same physical identity service

- Extend `backend/src/storage/mod.rs` block-device collection with WWN, transport, rotation,
  protocol/form-factor evidence, and a testable JSON parser. Keep Storage's live operational shape
  separate from persisted observations.
- On authenticated local device refresh, feed one full local snapshot into CMDB correlation under
  the local system resource and join each returned physical device to its canonical asset UUID/ID.
- Do not identify a disk by path, filesystem UUID, mountpoint, or partition UUID.
- Extend `/api/storage/devices` response and `frontend/src/api/types.ts` with an optional compact
  CMDB reference. Storage remains usable when CMDB reconciliation fails; return bounded warning
  metadata rather than hiding live devices.
- Keep SMART, mount, unmount, format, fstab, RAID, and paths on existing Storage endpoints and
  safety boundaries. CMDB detail links to those views but does not duplicate/forward mutations.
- Add tests proving local refresh and remote agent evidence resolve through the same matcher and
  return one asset for path changes or later host movement.

## 11. Add typed frontend clients and durable CMDB read hooks

- Add CMDB contracts to `frontend/src/api/types.ts` and all approved request methods to
  `frontend/src/api/client.ts` with encoded selectors and bounded query construction.
- Add `frontend/src/hooks/useCmdbRecords.ts` over the existing bounded-polling/event primitive for
  asset lists, detail, discoveries, settings, catalogs, locations, relationships, observations, and
  history.
- Lists invalidate broadly for any `cmdb.*` event with asset/discovery identity; details invalidate
  only for the exact selected resource UUID after the first authoritative response resolves an
  alias selector.
- Preserve J0 ready barriers, full HTTP recovery, one-active/one-pending burst coalescing,
  visibility recovery, stale confirmed state, fixed foreground deadline, and manual refresh.
- Add hook tests for ready/disconnect/gap behavior, exact versus broad invalidation, stale read
  preservation, alias-to-UUID stabilization, and events never mutating local records directly.

## 12. Build shared Assets surfaces for Tower and Void Mode

- Add focused shared views under `frontend/src/components/cmdb/` for inventory table/filtering,
  detail sections, discovery decisions, asset form, relationship editor, location tree, and read
  state notices.
- Add routed pages `frontend/src/pages/Assets.tsx`, `AssetDetail.tsx`, and `CmdbSettings.tsx` for
  `/assets`, `/assets/discoveries`, `/assets/:selector`, and `/settings/cmdb`.
- Add one thin AIOS native panel under `frontend/src/aios/panels/assets.tsx` reusing the shared
  inventory/detail components rather than implementing a second data flow.
- Add Assets to `Sidebar.tsx`, `AiosDock.tsx`, `AiosLayout.tsx`, `navConfig.ts`, icon registries,
  command navigation, and customization defaults under the Resources group. Keep persisted user
  navigation forward-compatible by relying on the existing default-item merge.
- Implement the approved Inventory columns/search/filters, detail Overview/Live/Relationships/
  History/Notes sections, manual create/edit/retire/restore, Discoveries decisions, identifier
  preview/settings, data-driven classes/types, and basic location hierarchy.
- Gate edit/config controls by role in addition to server enforcement. Operators see read-only
  inventory; lower roles receive route guards and no nav entry.
- Add `frontend/src/pages/Assets.test.tsx` and shared-view tests for filters, selector navigation,
  role gating, stale/error states, discovery decisions, rename alias messaging, retirement, and
  safe rendering of provider metadata.

## 13. Link Storage and asset detail without adding remote mutations

- Add an Asset ID column/link to the existing local Storage device table and keep rows functional
  when no CMDB reference exists.
- Add Manage in Storage on disk detail only for a current local observation with a live path.
- Render remote Linux/Windows path/disk number, host, health summary, and last-seen provenance as
  read-only Live data.
- Never surface mount/format/wipe/SMART actions for a remote disk through the observation agent.
- Ensure any existing local destructive Storage action retains its present auth, confirmation,
  denylist/approval, audit, and operation-boundary behavior.
- Add frontend tests for linked/unlinked local devices, remote read-only detail, stale observation,
  and correct route encoding.

## 14. Update documentation and operational packaging guidance

- Update `docs/api.md` with CMDB selectors, pagination/search, CRUD, discoveries, events,
  relationship/history behavior, and node inventory protocol.
- Update `README.md` and `ROADMAP.md` with the canonical asset identity boundary and mark the
  checkpoint complete only after the cross-platform disk movement acceptance test passes.
- Add an agent deployment document covering LAN HTTPS, optional CA, systemd foreground service,
  Windows Service supervision, token/state locations, enrollment/revocation, optional operator-
  managed WireGuard, and offsite backup-node reachability.
- Document that the first agent is observation-only and remote Storage mutations are unavailable.
- Update public Storage/node documentation and any Odysseus/AI context inventory that enumerates
  canonical read APIs, without granting an API token scope not approved by the design.
- Write the final ignored handoff with commit IDs, test counts, supported collectors, packaging
  limitations, and the recommended next hardware/logical-asset slice.

## 15. Complete verification and repository checkpoint

- During each stage run focused DB/domain/API/agent/collector/frontend tests and format only touched
  Rust files when repository-wide formatting would expose unrelated drift.
- Run `cargo test --all-targets --all-features` and
  `cargo clippy --all-targets --all-features -- -D warnings`.
- Run Windows/Linux target `cargo check` commands supported by installed targets/CI; record any
  local unavailable target explicitly while keeping CI authoritative.
- Run frontend `npm test`, `npm run type-check`, `npm run lint`, and `npm run build`.
- Run `scripts/check-schema-migration-ownership.sh`, `scripts/check-repository-hygiene.sh`, local
  `gitleaks git --no-banner` when installed, `git diff --check`, and staged diff checks.
- Exercise one real or fixture-backed end-to-end scenario: unknown strong-identity disk on Linux,
  removal, then the same disk on Windows, with one UUID/asset ID and preserved relationship/history.
- Commit implementation checkpoints locally without pushing.
- As required by repository hygiene, remove the tracked design and implementation plan from the
  final implementation tree while preserving their ignored local copies for handoff context.
- Confirm a clean branch and record the final ahead-of-origin state.

## Acceptance matrix

| Requirement | Evidence |
|---|---|
| One canonical identity spans CMDB and operations | Resource-FK schema and adoption tests |
| Human IDs are safe, configurable, and renameable | Concurrent allocation and alias-resolution tests |
| Discovery cannot overwrite administrator inventory | Observation precedence tests |
| Trusted strong identities auto-register safely | Correlation and discovery-policy tests |
| Weak/conflicting evidence never guesses | Discoveries review tests |
| Missing hardware persists | Full-snapshot removal tests |
| Linux/Windows movement creates no duplicate | Cross-platform acceptance test |
| Agents are outbound and narrowly authenticated | Runtime and real-router token tests |
| Local Storage and remote agents share correlation | Shared-service integration tests |
| Existing Docker/Proxmox/nodes are not duplicated | Idempotent adoption tests |
| CMDB changes are durable and auditable | Atomic event/audit rollback tests |
| Tower/Void show authoritative recoverable state | Shared-view and SSE fallback tests |
| Remote observation does not create a mutation bypass | Route/source inventory and UI tests |
