# Linux inventory upload and reconciliation

The enrolled agent inventory contract is `POST /api/nodes/:node_id/inventory`.

The request body is the versioned `InventorySnapshotV1` JSON produced by the
Linux collector. The node must be approved and agent-capable, and the bearer
token is hashed and matched to the path `node_id`; tokens from another node,
revoked nodes, and missing credentials are rejected before ingestion.

The node must already have one canonical CMDB host resource whose
`resources.node_id` equals the path node ID. The upload endpoint never accepts
a caller-selected `resources.id`; it derives the source host resource from the
trusted node binding.

Snapshots are replay-safe. Reposting the same `snapshot_id` with identical
content returns the completed result with `replayed: true` and does not create
a second snapshot. Reusing an ID for different content returns a bounded
conflict response; an identical upload whose first ingestion is still in
progress also returns a bounded processing conflict. Invalid schema, oversized
bodies (over 4 MiB), and unknown schema versions are rejected without CMDB
mutation.

Reconciliation persists the snapshot and observations transactionally. Strong
identity evidence is linked deterministically; weak or ambiguous evidence is
retained for review. Missing observations are marked missing only during a
successful non-empty snapshot convergence. Administrator-owned CMDB fields
(name, description, notes, lifecycle, condition, location, and ownership) are
not overwritten by discovery evidence. Durable audit and event records carry
correlation evidence; events are signals, not authoritative CMDB state.

Operational qualification is not established by the router tests. C3-03 still
owns supervised upload scheduling, bounded backoff, service installation,
outage/restart recovery, upgrade, and rollback. Runtime Linux collection and
controller outage tests remain required before release claims.
