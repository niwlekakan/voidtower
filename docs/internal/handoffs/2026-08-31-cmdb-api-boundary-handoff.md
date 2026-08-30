# CMDB Canonical API Boundary Handoff

Date: 2026-08-31
Status: Domain foundation through observations/discoveries complete locally; API slice ready
Branch: `dev`
Branch state before this handoff: twenty-eight commits ahead of `origin/dev`; nothing was pushed

## Outcome so far

CMDB plan steps 1 through 5 are now implemented below the HTTP boundary. The canonical resource
UUID remains the only asset identity. Catalogs, configurable human identifiers, manual asset
lifecycle, locations, generic relationships, provider-neutral physical identity correlation,
snapshot persistence, trusted-agent registration, missing convergence, and discovery decisions all
commit their state, events, and audit records transactionally.

No CMDB HTTP routes, inventory upload route, frontend surfaces, platform collectors, or
existing-resource adoption have been added yet. The next coherent slice is plan step 6: mount the
existing services behind the canonical CMDB/node-inventory API and authorization boundary.

## New commits

- `6cd3c45 feat(cmdb): add locations and asset relationships`
- `04aec10 feat(cmdb): add observation correlation and discoveries`
- `9f5de96 fix(cmdb): bound inventory snapshot fingerprints`

Earlier approved design and plan:

- `docs/internal/specs/2026-08-30-cmdb-asset-registry-foundation-design.md`
- `docs/internal/plans/2026-08-30-cmdb-asset-registry-foundation-implementation.md`

Migration `0003` remains unchanged and immutable. This slice required no `0004`.

## Location and relationship services

`backend/src/cmdb/locations.rs` provides create/list/get/update-move/delete plus caller-owned
transaction helpers. It trims and bounds fields, derives paths with recursive queries, enforces a
sixteen-level hierarchy, rejects missing parents/self-parenting/descendant cycles, protects
referenced locations from deletion, and writes audit rows without inventing location events.

`backend/src/cmdb/relationships.rs` provides start/end/list-by-asset plus caller-owned helpers. It
requires enabled relationship types and CMDB asset endpoints, canonicalizes bounded object
metadata, rejects self/duplicate edges, retains ended history, treats repeated end as idempotent,
and atomically moves `installed_in` relationships. Start/end facts are appended to both endpoint
histories using the approved event names and one correlation ID. Public starts use bounded SQLite
retry for deferred-transaction contention.

## Correlation service

`backend/src/cmdb/correlation.rs` is the only physical-identity matcher. Providers should submit
evidence to this module rather than selecting a resource UUID themselves.

It currently:

- normalizes validated WWN, NVMe UUID/EUI, serial, model, serial-plus-model, provider-scoped ID,
  and hardware UUID evidence;
- excludes paths, Linux device names, Windows disk numbers, and drive letters from identity;
- matches exact WWN, then NVMe UUID/EUI, then a unique serial-plus-model composite;
- returns explicit matched, unmatched-strong, or review outcomes;
- sends weak, malformed, ambiguous, and contradictory evidence to review; and
- treats only WWN/NVMe/serial-plus-model as auto-registerable physical identity.

Strong database uniqueness still applies to the identity kinds frozen in migration `0003`.
Serial-plus-model remains intentionally non-unique in storage so duplicated vendor evidence can be
reported as ambiguity rather than rejected during ingestion.

## Observation and discovery service

`backend/src/cmdb/observations.rs` exposes:

- `ingest`
- `list_discoveries`
- `get_observation`
- `ignore_discovery`
- `link_discovery`
- `register_discovery`

`ingest` requires an existing CMDB host profile as `source_resource_id`. For provider `agent`, the
resource's `resources.node_id` must match an approved, agent-capable enrolled node. The service
checks this trust even for replay; node revocation therefore invalidates uploads rather than
allowing cached replay access. Other provider ingestion is not automatically trusted unless the
identifier policy is `automatic`.

The service inserts the snapshot idempotency row before reconciliation inside the same write
transaction. A completed exact replay returns the stored result with `replayed=true` and no new
side effects. Reusing a snapshot UUID with different content is a conflict. Failure rolls back the
processing row and leaves the same snapshot UUID retryable.

Snapshots are canonicalized for fingerprinting with an explicit four-MiB bound. Entity count,
identity count, all key/string fields, JSON depth/value/collection sizes, and persisted JSON are
also bounded. Known capacity, rotation, interface, protocol, and form-factor attributes are
normalized. `M.2` is a form factor rather than an NVMe inference: SATA and NVMe M.2 assets retain
different protocol subtypes.

An exact strong match links the observation. Unmatched strong physical-disk evidence auto-registers
only under the approved policy and trust conditions and only when disk type can be classified from
rotation/protocol evidence. Registration creates the resource/profile/asset ID, identities,
observation, placement relationship, events, and audits in one transaction. Serial-only and other
weak evidence remains in review even from a trusted agent.

Full snapshots mark omitted linked observations missing without deleting assets. Placement ends
only when no current observation for that asset remains on the source host. Evidence on a new host
moves placement immediately; a later old-host snapshot cannot restore it. Administrator lifecycle
and descriptive fields are not updated during subsequent observations or explicit discovery links.

Ignore decisions are idempotent and suppress the same material evidence fingerprint. Runtime-only
changes such as a device path do not reopen review. Identity/attribute changes do. Register and
edit-and-register share `RegisterDiscoveryInput`; link-existing attaches normalized evidence while
preserving administrator fields. Ambiguous/contradictory or newly matched evidence cannot be
registered as a duplicate asset.

## Durable contracts frozen by tests

Relationship events:

- `cmdb.asset.relationship_started.v1`
- `cmdb.asset.relationship_ended.v1`

Observation/discovery events:

- `cmdb.asset.observation_linked.v1`
- `cmdb.asset.observation_state_changed.v1`
- `cmdb.discovery.ignored.v1`

Auto-registration reuses `cmdb.asset.created.v1`. Relationship movement events remain duplicated
onto the two endpoint resource histories and share the snapshot/action correlation ID.

## Verified state

After the observation/discovery checkpoint:

- `cargo test cmdb:: --all-features`: 37 passed
- focused canonical JSON tests: 3 passed
- simultaneous two-host evidence repeatedly left one asset and one active placement
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `scripts/check-schema-migration-ownership.sh`: passed
- staged and unstaged diff checks: passed

The full backend suite was not rerun. Do not report a new full-suite count until it has been run.
No frontend commands were needed because this checkpoint changed no frontend code.

## Next slice: canonical API and authorization boundary

Implement plan step 6 without adding platform collectors or frontend work in the same checkpoint.

1. Add a focused `backend/src/api/cmdb/` module tree rather than a single large handler file.
2. Mount the approved session routes for assets, catalogs, identifier settings, locations,
   relationships, discoveries, and observation/history detail.
3. Mount `POST /api/nodes/:id/inventory` behind node bearer authentication. Resolve the source host
   by canonical resource UUID/`resources.node_id`; never accept an asset UUID chosen by the agent.
4. Require administrator authorization for configuration and inventory mutations. Keep read-only
   routes consistent with the approved session/RBAC policy.
5. Map domain validation/not-found/conflict errors consistently to `400`/`404`/`409`; do not leak
   internal SQL or identity details.
6. Enforce request-body limits before JSON extraction, including the four-MiB inventory bound.
7. Add route-level tests for authorization, node-token isolation, replay, stale fingerprints,
   selector behavior, pagination bounds, and stable response/error shapes.
8. Finish the deferred general asset patch/search/tag/history behavior from plan step 3 only where
   required by the canonical API; do not start collectors or provider adoption in this commit.

Existing-resource adoption is still plan step 9. Until it is implemented, inventory-route tests
must explicitly seed/adopt a host profile. Do not create a competing host UUID in the upload
handler. If production routing needs host adoption before step 9, stop and move the approved
adoption service forward as a separate coherent checkpoint rather than embedding ad-hoc creation
in the handler.

## Architectural cautions

- `resources.id` remains the only asset UUID.
- Agents submit evidence and never choose a linked asset/resource UUID.
- Inventory writes are direct CMDB CRUD; provider/destructive actions stay on the typed
  job/approval boundary.
- Runtime observations never overwrite administrator lifecycle or descriptive fields.
- Only a completed full snapshot proves omission; upload/heartbeat failure does not.
- Keep migration `0003` immutable and add `0004` only for a genuine new schema requirement.
- Reuse transaction-aware audit/event helpers for any new domain mutation.
- Format focused leaf files only; avoid recursive rustfmt drift through `main.rs` or module roots.
- Preserve unrelated worktree changes and do not push.

## Suggested API checkpoint verification

From `backend/`:

```text
cargo test cmdb:: --all-features
cargo test api::cmdb --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

From the repository root:

```text
scripts/check-schema-migration-ownership.sh
git diff --check
```

Commit the coherent API slice locally with a focused message and do not push.
