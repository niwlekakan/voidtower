# CMDB / Asset Registry Vertical Foundation

Date: 2026-08-30
Status: Approved
Scope: Resource-backed CMDB, Linux and Windows inventory agents, physical-disk correlation, and
initial Tower/Void inventory integration

## Purpose

Make VoidTower's existing resource UUID the permanent identity for physical and logical assets.
Discovery reports what currently exists, the CMDB records what the thing is, and operational
providers retain ownership of what can be done to it. The first delivery must prove that boundary
with manual inventory and a real physical-disk workflow across local, remote Linux, and remote
Windows hosts.

An unknown disk with a strong identity is registered once, receives a configurable human asset ID,
and is linked to its current host. Removal makes it missing rather than deleting it. Reinsertion on
another Linux or Windows host reuses the same resource UUID and human ID, updates the active host
relationship, and preserves history. Weak or conflicting evidence enters a review inbox rather than
creating a guessed identity.

This slice also gives existing managed-node, Docker, and Proxmox resources an explicit CMDB
projection without duplicating them. Jobs, capabilities, tags, durable events, policy, approvals,
and provider mutations continue to use the J0 resource and operation contracts.

## Fixed decisions

- `resources.id` remains the immutable UUID for assets and every related foreign key.
- CMDB is an optional projection of a resource, not a parallel asset identity system.
- Human asset IDs are unique, configurable, renameable, and never primary keys.
- Previous human IDs remain resolvable aliases. Changing identifier settings never silently
  renames existing assets.
- Classes and types are data-driven and administrator-extensible.
- Administrator-owned inventory data and provider-owned observations are persisted separately.
- Existing durable events are the CMDB history and frontend invalidation source.
- The first production agent runtime supports Linux and Windows host and physical-disk inventory.
  Its collectors contain no architecture-specific assumptions beyond the platform APIs they use.
- Agents make outbound HTTPS requests and expose no action listener.
- WireGuard is optional transport configured by the operator. LAN, reverse-proxy, tunnel, and
  offsite VPN deployments all use the same HTTPS/node-token protocol.
- Trusted enrolled agents auto-register only unmatched strong hardware identities. Weak,
  incomplete, or conflicting identities require review.
- Remote disk mutation is not part of this slice.
- Do not push local commits.

## Approaches considered

### Recommended: resource-backed CMDB projection

Add CMDB-specific tables keyed by `resources.id`. Existing jobs, capabilities, provider aliases,
tags, durable events, Docker observations, Proxmox observations, and managed nodes immediately
refer to the same object. CMDB concerns stay isolated without losing canonical identity.

### Rejected: independent assets with a resource bridge

A separate asset UUID would isolate CMDB tables but create two competing identities. Every job,
event, tag, merge, alias, and provider observation would require bridge resolution and failure
handling. This contradicts the J0 resource contract and makes duplicate physical assets more
likely.

### Rejected: add all CMDB columns directly to `resources`

This initially simplifies reads but forces firewall rules, update targets, backup definitions, and
other operational-only resources to carry irrelevant inventory fields. Observation, provenance,
relationships, type administration, and counters still require separate persistence, so widening
the core table does not remove meaningful complexity.

## Existing architecture and integration points

Migration `0002_operation_contracts.sql` owns `resources`, globally scoped `resource_aliases`,
resource capabilities, jobs, approvals, and durable events. Provider observation already preserves
resource UUIDs through namespace, scope, and native aliases. `resources.id` is referenced by jobs
and events with restrictive deletion rules.

The current Storage module reads local block devices from `lsblk`, exposes live device paths and
SMART/mount operations, and persists no device identity. It has serial/model/vendor data but does
not currently request WWN, NVMe persistent identifiers, transport, rotation, or physical form
factor. Storage remains the operational owner and becomes a producer/consumer of CMDB identity.

Node enrollment already issues a node UUID and a narrowly scoped hashed token used for heartbeat.
It also returns optional WireGuard client configuration. The `--agent` flag currently changes only
configuration; there is no production collector, upload loop, or action protocol. This slice turns
that flag into an outbound inventory runtime and adds a first-class enrollment command.

The frontend uses routed Tower pages, reusable AIOS panels, a customizable navigation model, local
request state, and a shared cursor-resumable durable-event multiplexer. CMDB pages must reuse those
patterns and treat SSE only as invalidation.

## Persistence model

Add numbered migration `0003`; do not modify migrations `0001` or `0002`.

### Asset profiles

`cmdb_assets` has a one-to-one primary and foreign key to `resources.id`. It owns:

- current human `asset_id` with a global unique constraint;
- class and type references plus optional subtype;
- friendly name, description, manufacturer, model, serial number, and part number;
- CMDB lifecycle state, discovery state, and condition;
- optional location reference;
- first-seen, last-seen, created, and updated timestamps;
- bounded JSON metadata and notes; and
- an update revision used for optimistic concurrency.

The existing three-state `resources.lifecycle_state` remains the coarse operational boundary used
by jobs. CMDB lifecycle is richer. Retired/disposed/lost CMDB states make the resource unavailable
or retired through an explicit mapping; discovery does not silently change administrator-owned
lifecycle.

Initial lifecycle values are `unknown`, `new`, `inventory`, `testing`, `available`, `reserved`,
`deployed`, `maintenance`, `degraded`, `quarantine`, `wipe_pending`, `wiping`, `wiped`, `retired`,
`disposed`, and `lost`. Discovery values are `online`, `offline`, `missing`, `manual`, `unmanaged`,
`ignored`, and `stale`. Initial condition values are `unknown`, `new`, `good`, `fair`, `poor`,
`damaged`, and `failed`. API/domain validation owns these values in this slice so a later explicit
status-administration design can replace them without embedding provider logic in SQL checks.

Manual assets create a resource UUID and CMDB profile in one transaction. Existing operational
resources acquire a profile on adoption and keep their UUID, aliases, revision, capabilities, jobs,
and events.

### Classes and types

`cmdb_classes` and `cmdb_types` store stable keys, display labels, descriptions, built-in status,
enabled status, and timestamps. Types reference a class. Built-ins are seeded idempotently.

Initial class keys are `hw`, `net`, `sys`, `svc`, `data`, `dev`, `sec`, `iot`, `pwr`, `per`, `loc`,
and `lic`. Initial physical storage types include `hdd`, `ssd`, `ssd25`, `ssdms`, `ssdm2`,
`ssdpcie`, `ssdu2`, `ssdu3`, `usb`, `sd`, `msd`, `opt`, and `tape`. `ssdm2` denotes the M.2 form
factor; protocol and interface remain observation/metadata fields and do not make M.2 synonymous
with NVMe.

Built-ins may be disabled only when no active asset depends on them. Custom keys use a validated,
bounded portable grammar so they remain safe in identifiers and URLs.

### Human identifiers and aliases

Dedicated settings store prefix, template, separator, number width, starting number, counter scope,
and case behavior. The shipped default is:

```text
VT-{class}-{type}-{number}
```

with width four, start one, `0000` reserved, and class-plus-type counter scope. Templates are parsed
against an allowlist of fields; rendered values are length-bounded and checked for uniqueness.

`cmdb_identifier_counters` owns one next value per configured scope. Allocation and asset insertion
run under one SQLite write transaction so concurrent discoveries cannot receive the same ID.
Changing settings affects future allocations only.

The current ID is stored on `cmdb_assets`. It is also resolvable through the existing
`resource_aliases` table using namespace `cmdb.asset_id` and global scope. Renaming inserts the old
ID as a retained alias, inserts the new current ID as another alias, and updates
`cmdb_assets.asset_id` transactionally. The old alias row is never rewritten or removed by a
rename. An alias can never refer to two resources. UUID, current ID, and retained CMDB alias are
accepted asset selectors.

### Stable identities and observations

`cmdb_asset_identities` stores normalized identity kind/value pairs, confidence, provenance, and
first/last-observed timestamps. Globally strong kinds such as WWN and suitable NVMe persistent
identifiers are uniqueness-constrained. Composite serial-plus-model values are normalized before
matching. Serial-only and provider-native identifiers are retained as evidence but are not assumed
globally unique.

`cmdb_observations` stores a provider and source-node scoped entity key, optional linked resource
UUID, schema version, normalized identity evidence, attributes, runtime state, health, provider
timestamp, server receive time, last-seen time, state, and snapshot fingerprint. JSON fields are
bounded and structurally validated. Runtime values such as `/dev/sdc`, Windows disk number,
temperature, mount state, IP address, and current host never overwrite administrator inventory
fields.

Unlinked observations are the Discoveries inbox. Ignoring a discovery records the decision and its
fingerprint so an unchanged scan does not recreate inbox noise. Materially changed evidence may
reopen review.

### Relationships and locations

`cmdb_relationship_types` stores stable key, display label, inverse label, built-in/enabled status,
and timestamps. `cmdb_relationships` links source and destination resource UUIDs through one of
those types and stores start/end timestamps, active status, and bounded JSON metadata. Initial
types include `contains`, `installed_in`, `connected_to`, `attached_to`, `hosts`, `hosted_on`,
`runs`, `runs_on`, `member_of`, `depends_on`, `stores`, `stored_on`, `backs_up`, `backed_up_by`,
`powers`, `powered_by`, `located_at`, `assigned_to`, and `managed_by`.

Attachment metadata can later describe drive bays, ports, slots, or outlets without a schema
rewrite. Active relationship uniqueness prevents duplicate simultaneous edges. Moving a disk ends
the prior `installed_in` relationship before starting the new one.

`cmdb_locations` stores a UUID, optional parent UUID, name, description, and timestamps. Parent
validation prevents self-parenting and cycles. Assets reference locations by UUID; display paths are
derived. Basic hierarchy CRUD is in scope, but graph editing and movement visualization are not.

### History, audit, and tags

CMDB state transitions append versioned events through `operations::events::append` in the same
transaction as their data changes. Initial concepts are:

- `cmdb.asset.created.v1`, `updated.v1`, `retired.v1`, and `restored.v1`;
- `cmdb.asset.identifier_changed.v1`;
- `cmdb.asset.observation_linked.v1` and `observation_state_changed.v1`;
- `cmdb.asset.relationship_started.v1` and `relationship_ended.v1`; and
- `cmdb.discovery.ignored.v1`.

Relationship changes emit correlated per-asset events so either endpoint has complete indexed
history without replacing the single-resource field in `EventEnvelopeV1`. Human and node mutations
also insert an audit row in the same transaction. Events contain bounded non-secret summaries, not
raw provider diagnostics.

Existing tags attach with `resource_type=asset` and the canonical resource UUID.

## Agent runtime

### Lifecycle and configuration

The existing binary gains an `agent enroll` command that accepts the server HTTPS URL, pairing
code, display name, and device type. It calls the existing enrollment flow, persists the returned
node UUID and scoped token, and records the server/optional CA configuration with restrictive
platform permissions.

Enrollment adds an explicit optional WireGuard-provisioning request. The new agent command leaves
it off by default, so a controller without a configured WireGuard service can still enroll LAN or
independently tunneled nodes. Existing clients retain their current request default and response
shape; when WireGuard is requested and available, the response still includes its client
configuration.

`voidtower --agent` runs a foreground Tokio loop suitable for systemd, Windows Services, container
supervision, or manual execution. It does not initialize the controller database, frontend,
operation workers, or mutation APIs. It sends lightweight heartbeats independently of inventory
snapshots and retries transport failures with bounded exponential backoff and jitter.

The agent trusts normal platform certificate roots plus an optional configured CA certificate.
Certificate verification is on by default. LAN nodes use a reachable local URL. Offsite nodes may
use an operator-managed public reverse proxy, tunnel, or WireGuard route. The agent neither installs
nor reconfigures WireGuard.

Node deletion/revocation invalidates both heartbeat and inventory uploads. The node token is never
accepted by session, bearer-scope, resource mutation, or action routes.

### Versioned snapshot protocol

Agents POST a versioned, size-bounded full snapshot to a node-specific inventory endpoint. A
snapshot includes a client-generated UUID, collector version, platform, host identity, collection
time, and normalized entity records. Server receive time is authoritative for liveness; client time
is retained only as provenance.

The server records the node and snapshot UUID before reconciliation. Replaying a completed
snapshot returns its prior result and creates no new identifiers, relationships, events, or audit
rows. An interrupted snapshot transaction is safe to retry.

A successfully completed full snapshot is evidence that previously active observations omitted
from that snapshot are missing. Upload failure or heartbeat loss alone is not absence evidence and
does not remove hardware. Missing assets remain persistent, their observation state changes, and
their active host relationship ends.

When a strong identity appears on a different host before the old host submits a removal snapshot,
linking the new observation transactionally ends any other active `installed_in` relationship for
that physical asset. A later old-host snapshot is therefore idempotent and cannot restore the stale
attachment.

### Platform collectors

Collectors implement one internal platform-neutral contract. Tests feed collectors captured JSON
fixtures rather than invoking host commands.

Linux uses `lsblk` with explicit JSON/byte fields including path, type, model, vendor, serial, WWN,
UUID, transport, rotation, mount/filesystem state, and device relationships. `/sys` supplies
available fallbacks and stable device properties. Loop, RAM, temporary virtual block devices,
partitions as independent physical assets, and ephemeral mounts are excluded from registration.

Windows uses built-in PowerShell/CIM storage APIs to collect system UUID and physical disk, disk,
and association data. The command is constant and receives no user-controlled script fragments.
Disk numbers and drive letters are runtime observations, never identity. The collector tolerates
missing optional properties and reports bounded diagnostics. It supports currently supported
Windows desktop and server releases with their built-in PowerShell/CIM facilities.

Both collectors initially emit the host and physical disks. CPU, motherboard, DIMM, GPU, NIC,
PCI/USB, filesystem-as-asset, and other collectors are follow-up additions using the same protocol.

## Correlation and discovery policy

Correlation is one server-side service shared by remote snapshot uploads and local Storage refresh.
Agents and providers may submit evidence but may not select a resource UUID or allocate an asset ID.

Disk matching priority is:

1. normalized WWN;
2. NVMe controller/namespace persistent identifiers suitable for device identity;
3. normalized serial plus model;
4. serial alone; and
5. provider-native stable evidence.

Levels one through three are strong only after validation. An exact single strong match links the
observation. An unmatched strong identity from a trusted enrolled node creates a disk asset,
identities, observation, and `installed_in` relationship transactionally. HDD/SSD classification
uses rotation, transport, protocol, and form-factor evidence without treating M.2 as NVMe.

Serial-only, missing, malformed, multiply matching, or mutually contradictory evidence creates or
updates an unlinked discovery. It never auto-merges or silently changes existing identities. Review
may register a new asset, link an existing one, edit-and-register, or ignore the evidence.

The default discovery policy is trusted providers: enrolled nodes with valid scoped tokens may
auto-register strong identities. Other providers and weak evidence require review. Off, review
first, trusted providers, and automatic modes are represented in settings, but automatic never
overrides a detected identity conflict.

## Existing-resource adoption

Startup initialization projects suitable existing resources idempotently:

- the local VoidTower system and enrolled nodes become `sys/host` assets;
- Docker container resources become service/application assets;
- Proxmox QEMU and LXC guest resources become VM and container assets; and
- Proxmox storage resources become data/storage-pool assets.

Adoption preserves resource UUIDs and provider aliases. Human IDs are allocated transactionally in
a deterministic order for existing rows. It adds no provider mutation and does not infer rich
inventory values absent from current evidence. Operational-only resources such as firewall rules,
update targets, and approval records do not automatically become assets.

Proxmox physical disks use the same correlation service when strong identity data is available.
Provider paths alone remain scoped aliases/observations and cannot establish physical identity.

## API contract

Canonical session routes live under `/api/cmdb`:

```text
GET    /api/cmdb/assets
POST   /api/cmdb/assets
GET    /api/cmdb/assets/:selector
PATCH  /api/cmdb/assets/:selector
DELETE /api/cmdb/assets/:selector
POST   /api/cmdb/assets/:selector/restore

GET    /api/cmdb/assets/:selector/history
GET    /api/cmdb/assets/:selector/observations
GET    /api/cmdb/assets/:selector/relationships
POST   /api/cmdb/assets/:selector/relationships
GET    /api/cmdb/assets/:selector/aliases
POST   /api/cmdb/assets/:selector/aliases
DELETE /api/cmdb/assets/:selector/aliases/:alias
DELETE /api/cmdb/relationships/:id

GET    /api/cmdb/discoveries
POST   /api/cmdb/discoveries/:id/register
POST   /api/cmdb/discoveries/:id/link
POST   /api/cmdb/discoveries/:id/ignore

GET    /api/cmdb/classes
POST   /api/cmdb/classes
PATCH  /api/cmdb/classes/:key
DELETE /api/cmdb/classes/:key
GET    /api/cmdb/types
POST   /api/cmdb/types
PATCH  /api/cmdb/types/:key
DELETE /api/cmdb/types/:key
GET    /api/cmdb/locations
POST   /api/cmdb/locations
PATCH  /api/cmdb/locations/:id
DELETE /api/cmdb/locations/:id

GET    /api/cmdb/settings
PATCH  /api/cmdb/settings

POST   /api/nodes/:id/inventory
```

Every shipped route must be registered in the real router and source inventory. List uses bounded
`limit`/`offset` pagination, free-text search, and explicit filters for class, type, lifecycle,
discovery state, host, location, and tag. Search covers current/old asset IDs, name, serial, WWN,
manufacturer, model, tags, locations, and hosts. Structured query shorthand is parsed into bound
SQL values and never interpolated.

`DELETE` retires rather than physically erases an asset. Restore is explicit. Purge, merge, split,
bulk identifier migration, QR/PDF labels, custom fields, and custom relationship-type UI are not
part of this slice.

Node agents retain heartbeat and gain a versioned inventory upload route under `/api/nodes/:id`.
The endpoint uses only constant-time verification of that node's token and rejects uploads for any
other node identity.

## Authorization and error behavior

Owner and admin sessions can create, edit, retire, restore, link, ignore, and configure CMDB data.
Operator sessions can read inventory, observations, relationships, and history. Member, guest,
demo, viewer, unknown, and unauthenticated sessions cannot read household inventory by default.
CMDB does not introduce another auth system.

Node tokens can call only their own heartbeat and inventory endpoints. They cannot use the generic
bearer-session bridge or canonical action boundary. Input limits apply before deserializing large
collections, and each snapshot has entity-count and string/JSON size bounds.

Invalid templates, identifiers, class/type keys, selectors, relationships, locations, and
observations return structured `400 bad_request`. Duplicate human IDs, stale update revisions, and
relationship races return `409 conflict`. Missing records return `404`. Correlation conflicts are
successful discovery ingestion with review status, not server errors.

Provider diagnostics, payloads, metadata, notes, and event summaries use explicit limits and
existing secret-pattern redaction where relevant. No observation can submit, approve, retry, or
execute a mutation.

## Frontend design

Add Assets to the Resources navigation group in Tower and Void Mode. Shared view components back
the routed page and AIOS panel.

Tower routes are `/assets`, `/assets/discoveries`, and `/assets/:selector`; CMDB configuration is
at `/settings/cmdb`. The more specific Discoveries route is registered before the selector route.

Initial surfaces are:

- Inventory with search/filter and Asset ID, name, class/type, model, lifecycle, discovery state,
  host, location, health, last seen, and tags;
- asset detail with Overview, Live, Relationships, History, and Notes;
- Discoveries with register, link existing, edit-and-register, and ignore actions;
- manual create/edit/retire/restore; and
- CMDB settings with identifier preview, counters, discovery policy, classes, types, and basic
  location hierarchy management.

Unsupported future tabs are not rendered. A relationship graph, label designer, PDF generation,
merge UI, and custom-field builder are follow-up work.

The local Storage device table gains an Asset ID column linking to asset detail. Asset detail shows
the latest Storage observation and a Manage in Storage link only while a current local device path
exists. Remote Linux and Windows disks show live observation and host relationship data without
remote mutation controls. Storage remains authoritative for SMART, mount, format, RAID, and other
operations.

CMDB lists and details subscribe to relevant durable events through the existing shared
multiplexer. SSE remains invalidation only: ready/event/gap causes authoritative HTTP recovery,
event bursts coalesce, visibility return refetches, and bounded polling remains the fallback.

## Verification

### Persistence and identifiers

- Fresh and upgraded schemas include migration `0003`, match the golden schema, and pass integrity
  validation.
- Built-in class/type and existing-resource adoption are idempotent.
- Default identifiers allocate sequentially with independent class/type counters.
- Concurrent allocation cannot duplicate an identifier.
- Configuration changes do not rename existing assets.
- Renaming retains the old identifier and resolves it to the same resource UUID.
- Manual assets require no provider observation.

### Agent protocol and collectors

- Enrollment persists only the scoped node credentials with restrictive permissions.
- Node tokens cannot reach another node or any session, CMDB-admin, or action route.
- Snapshot replay is idempotent, partial transactions are retryable, and bounds fail closed.
- Linux `lsblk` and sysfs fixtures normalize persistent identities and exclude ephemeral devices.
- Windows PowerShell/CIM fixtures normalize the same contract and treat disk numbers/drive letters
  as runtime-only data.
- Transport backoff is bounded and a failed upload does not mark hardware missing.

### Correlation and lifecycle

- WWN and suitable NVMe identity changes in device path still match one asset.
- Serial-plus-model matching is normalized and serial-only evidence requires review when weak.
- Conflicting evidence creates a review discovery and does not mutate existing assets.
- A completed snapshot missing a disk preserves the asset, changes observation state, and ends the
  active host relationship.
- Moving a disk between Linux and Windows hosts preserves UUID, asset ID, aliases, identities,
  history, and tags while replacing the active relationship.
- M.2 SATA and M.2 NVMe observations both map to `ssdm2` with distinct interface/protocol metadata.

### API, authorization, events, and UI

- Real-router tests freeze every role boundary and node-token route.
- Source-inventory tests freeze all added endpoints and prove there is no mutation bypass.
- CMDB mutations, audit rows, and durable events commit or roll back together.
- Asset history is complete for both sides of relationship changes.
- Search uses bounded parameters and resolves UUID, current ID, and retained alias.
- Tower and Void views share list/detail behavior and role gating.
- Storage and asset-detail navigation use the same resource UUID.
- Ready, event, disconnect, gap, visibility, burst, and read-failure paths preserve authoritative
  HTTP recovery and bounded polling.

### Full gate

- `cargo test --all-targets --all-features`
- `cargo clippy --all-targets --all-features -- -D warnings`
- frontend `npm test`, `npm run type-check`, `npm run lint`, and `npm run build`
- schema migration ownership, repository hygiene, and `git diff --check`

## Delivery stages

Implementation is divided into reviewable commits without weakening the end-to-end acceptance
goal:

1. migration, domain model, identifier allocation, aliases, relationships, events, and tests;
2. canonical CMDB API, authorization, search, manual CRUD, discoveries, and settings;
3. agent enrollment/runtime, Linux and Windows collectors, upload protocol, and reconciliation;
4. existing-resource adoption, local Storage correlation, Tower/Void UI, and durable invalidation;
5. documentation, complete verification, and roadmap/handoff update.

Each stage preserves existing behavior and passes its focused tests. The final milestone is not
complete until the same disk can move between enrolled Linux and Windows hosts without creating a
duplicate asset.
