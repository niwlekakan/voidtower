# CMDB Locations and Relationships Handoff

Date: 2026-08-30
Status: Foundation complete locally; next domain slice ready
Branch: `dev`
Branch state before this handoff: twenty-four commits ahead of `origin/dev`; nothing was pushed

## Outcome so far

The first two CMDB/Asset Registry implementation checkpoints are complete. CMDB assets are optional
profiles over the existing canonical `resources.id` UUID rather than a competing inventory identity.
The database now has the catalogs, identifiers, observations, discoveries, locations, and generic
relationship foundations required by the approved design. The Rust domain layer can seed the
catalogs, allocate configurable human-readable asset IDs transactionally, create manual assets,
resolve UUID/current-ID/retained-ID selectors, rename assets without breaking old links, and retire
or restore them while writing durable events and audit records in the same transaction.

No CMDB HTTP routes, frontend surfaces, inventory agent, observation reconciliation, or provider
adoption have been added yet. The next slice should remain below the API boundary and finish the
location and relationship domain services.

## Relevant commits

- `4c0b088 docs(cmdb): design asset registry foundation`
- `7fa8ddb docs(cmdb): plan asset registry foundation`
- `acb2294 feat(cmdb): add asset registry schema`
- `c4a074e feat(cmdb): add catalogs and manual asset identity`

The approved design and full implementation plan are:

- `docs/internal/specs/2026-08-30-cmdb-asset-registry-foundation-design.md`
- `docs/internal/plans/2026-08-30-cmdb-asset-registry-foundation-implementation.md`

Both internal files are ignored by repository policy but are intentionally still tracked during
the implementation checkpoints. Remove their tracked copies only at the final CMDB implementation
checkpoint, while leaving the ignored local copies available for handoff context.

## Implemented persistence and contracts

`backend/migrations/0003_cmdb_asset_registry.sql` adds:

- `cmdb_classes` and `cmdb_types`
- `cmdb_relationship_types`
- `cmdb_locations`
- `cmdb_identifier_settings` and `cmdb_identifier_counters`
- `cmdb_assets`
- `cmdb_asset_identities`
- `cmdb_inventory_snapshots`
- `cmdb_observations`
- `cmdb_discovery_decisions`
- `cmdb_relationships`

Every asset-facing foreign key uses the canonical resource UUID. Human IDs are aliases in
`resource_aliases` under namespace `cmdb.asset_id` and global scope. The schema enforces current
asset-ID uniqueness and strong WWN uniqueness while intentionally allowing weak serial evidence to
repeat.

Migration `0003` has been committed and must now be treated as immutable. If the next slice exposes
a schema defect, add `0004`; do not edit the committed migration.

`backend/src/cmdb/contracts.rs` contains the first versioned domain/upload contracts, including:

- lifecycle, discovery, and condition enums with stable snake-case strings
- identifier configuration and counter-scope enums
- `AssetRecord`
- `IdentityEvidenceV1`, `HostObservationV1`, and `ObservedEntityV1`
- `InventorySnapshotV1` and its result shape

`backend/src/db/schema.rs`, the schema golden file, and migration tests now recognize migrations
1 through 3.

## Implemented domain services

`backend/src/cmdb/catalog.rs` seeds data-driven built-ins idempotently:

- Classes: `hw`, `net`, `sys`, `svc`, `data`, `dev`, `sec`, `iot`, `pwr`, `per`, `loc`, `lic`
- Initial storage/system/service/data types, including `hdd`, `ssds`, `host`, `vm`, `ct`, `app`,
  `db`, `pool`, and `bkp`
- The approved generic relationship types
- Default identifier settings using prefix `VT`, `-`, width 4, class/type counters, and trusted
  providers

`backend/src/cmdb/identifiers.rs` validates portable catalog keys, settings, templates, and output;
supports the approved `prefix`, `separator`, `class`, `type`, and `number` tokens; and allocates
counters atomically inside a caller-owned transaction. Configuration changes do not rewind an
existing counter.

`backend/src/cmdb/assets.rs` currently supports:

- bounded basic listing with `limit` and `offset`
- selector lookup in order: UUID, current human ID, retained CMDB alias
- manual creation over a new canonical resource UUID and current alias
- rename with an expected CMDB revision and retained old alias
- soft retirement and restore to `inventory`, with explicit coarse resource-state mapping
- transactional durable events and audit rows for those mutations

It does not yet implement the complete filtered search, general patch, tag joins, optional-alias
deletion, or complete history described by plan step 3. Those can be finished with the API slice;
they are not prerequisites for the location/relationship service.

`backend/src/audit/mod.rs` now provides transaction-aware `PendingAudit` plus `append`. Existing
callers retain the prior best-effort audit API.

## Verified state

Before the CMDB work, the full baseline passed with 351 backend unit tests, 2 golden-path tests,
and 25 frontend tests, plus frontend type-check, lint, and build.

After the schema checkpoint:

- focused database tests: 10 passed
- schema golden test: passed
- migration-ownership check: passed
- strict Clippy: passed

After the domain checkpoint:

- `cargo test cmdb:: --all-features`: 12 passed
- concurrent manual creation allocated distinct IDs
- rename retained the old selector
- independent counters, settings changes, retirement, restore, and idempotent seeds were covered
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `scripts/check-schema-migration-ownership.sh`: passed
- `git diff --check`: passed

The full backend suite was not rerun after `c4a074e`; do not report a new full-suite count until it
has been run.

## Next slice: location and relationship domain services

Keep this slice focused on plan step 4. Do not mount public routes or begin agent ingestion in the
same commit.

### Locations

Add `backend/src/cmdb/locations.rs` with caller-owned transactional helpers and public service
operations for create, list, get, update/move, and delete.

Required behavior:

1. Use UUID location IDs and bounded, trimmed names/descriptions.
2. Enforce the existing root-level name uniqueness and sibling semantics represented by the
   schema.
3. Require a referenced parent to exist.
4. Reject self-parenting and descendant cycles transactionally.
5. Enforce a fixed maximum hierarchy depth during create and move.
6. Derive display paths from the hierarchy; do not persist a second path identity.
7. Reject deletion while the location has children or is referenced by an asset.
8. Append bounded audit records in the same transaction as each mutation.
9. Freeze any new durable event names and payload versions in tests before the later API/UI begins
   consuming them. If location-only events are unnecessary for authoritative asset invalidation,
   prefer audit-only location mutations to inventing an unused stream contract.

Minimum tests:

- root and nested creation plus derived path
- duplicate root/sibling names
- missing parent and self-parent rejection
- multi-level descendant-cycle rejection
- maximum depth at both create and move
- deletion blocked by children and by asset references
- successful leaf deletion
- mutation rollback leaves neither state nor audit residue

### Relationships

Add `backend/src/cmdb/relationships.rs` with create/start, end, and list-by-asset operations.

Required behavior:

1. Require an enabled relationship type and two existing CMDB assets.
2. Reject every self-edge, matching the committed schema constraint.
3. Validate bounded canonical JSON metadata before insertion.
4. Enforce at most one identical active edge while preserving ended historical rows.
5. When starting `installed_in`, atomically end every other active `installed_in` relationship for
   the source asset before inserting its new host relationship.
6. Treat an already-ended edge as an idempotent end or a stable domain conflict; choose one explicit
   contract and freeze it in tests before API error mapping. Idempotent end is preferred for
   reconciliation retries.
7. Append `cmdb.asset.relationship_started.v1` and
   `cmdb.asset.relationship_ended.v1` to both endpoint histories. Each event must be anchored to
   that endpoint's resource UUID, and all events from one move must share a correlation ID.
8. Append audit records transactionally with relationship state and events.

Minimum tests:

- missing or disabled relationship type
- missing endpoint and self-edge rejection
- duplicate active edge rejection
- ended edge retained and a later equivalent edge allowed
- metadata size/type/canonicalization limits
- list-by-asset includes source and destination history
- `installed_in` movement from host A to host B leaves exactly one active host
- movement emits old-end and new-start events for both endpoints with one correlation ID
- repeated end behavior and concurrent movement race
- rollback leaves no relationship, durable event, or audit residue

Export both modules from `backend/src/cmdb/mod.rs`. Prefer small domain-specific error enums that a
later API layer can map consistently to `400`, `404`, and `409`; do not couple these services to
Axum response types.

## Architectural boundaries to preserve

- `resources.id` remains the only asset UUID. Never create a second canonical asset identity.
- A relationship endpoint is a CMDB asset profile, not an arbitrary provider alias.
- `installed_in` represents current physical/logical placement history; it does not authorize any
  operation on either endpoint.
- CMDB writes are direct inventory CRUD, while destructive/provider actions remain on the existing
  typed job/approval boundary.
- Durable events are invalidation/history facts, never a materialized state store.
- Administrator lifecycle and descriptive fields must not later be overwritten by observations.
- Linux and Windows agents will be outbound HTTPS clients. WireGuard remains optional and
  operator-managed for reaching secure services or offsite nodes; it is not required for ordinary
  LAN inventory.
- Strong identity from a trusted enrolled agent may later auto-register. Weak, malformed,
  ambiguous, or conflicting evidence must go to Discoveries rather than guessing.

## Implementation cautions

- Preserve all unrelated worktree changes if any appear after resumption.
- Do not edit migration `0003`.
- SQLite relationship movement must occur in one write transaction; do not implement
  end-then-start as separate public calls.
- Use the transaction-aware audit helper rather than the best-effort wrapper inside CMDB writes.
- Reuse the existing durable event append helper and its resource indexing conventions.
- Bound all strings, collection sizes, metadata bytes, hierarchy depth, list limits, and retry
  attempts before persistence.
- Format only focused leaf Rust files. Running `rustfmt` on `main.rs` or module roots recursively
  reformats unrelated established code and creates broad drift. That spill was encountered and
  safely reversed during the prior slice.
- Do not push.

## Suggested verification and checkpoint

Run, from the repository root:

```text
cargo test cmdb:: --all-features
cargo clippy --all-targets --all-features -- -D warnings
scripts/check-schema-migration-ownership.sh
git diff --check
```

Also run any narrowly named location/relationship tests while iterating. If the slice adds no
migration or frontend code, schema golden and frontend commands need not be repeated before its
checkpoint, though the full verification remains required at the final foundation checkpoint.

Commit the coherent slice locally as:

```text
feat(cmdb): add locations and asset relationships
```

After this slice, proceed to observation persistence, physical-identity correlation, trusted-agent
auto-registration, and the Discoveries decision service (plan step 5), then mount those services
behind the canonical CMDB/node-inventory API boundary (plan step 6).
