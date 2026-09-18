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

The bearer credential is verified before the endpoint parses JSON or applies
the 4 MiB application body limit. Missing or invalid credentials therefore
return `401 unauthorized`; authenticated bodies over 4 MiB return
`413 payload_too_large`.

Snapshots are replay-safe. Reposting the same `snapshot_id` with identical
content returns the completed result with `replayed: true` and does not create
a second snapshot. Reusing an ID for different content returns a bounded
conflict response; an identical upload whose first ingestion is still in
progress also returns a bounded processing conflict. The route validates the
schema version, UUID snapshot ID, bounded text fields, positive collection time,
host/entity keys, identity fields, entity count, and duplicate keys before
resolving the canonical host. Invalid schema, control characters, oversized
bodies (over 4 MiB), and unknown schema versions are rejected without CMDB
mutation.

A successful upload response is the complete `InventorySnapshotResultV1`
object; clients require all six fields and treat malformed JSON or missing
fields as a failed upload eligible for bounded retry. The response
`snapshot_id` must match the uploaded request after trimming the contract's
allowed surrounding whitespace; an acknowledgement for another snapshot is
rejected so a pending snapshot remains eligible for retry. The controller
returns the canonical trimmed ID:

```json
{
  "snapshot_id": "uuid",
  "replayed": false,
  "linked": 1,
  "registered": 0,
  "review_required": 0,
  "missing": 0
}
```

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

Enrollment lifecycle safety

An owner or administrator creates a short-lived pairing code, and the agent
submits it to `POST /api/nodes/enroll`. The controller claims the code
atomically before creating the node, so concurrent submissions result in one
successful enrollment and one `401 unauthorized`; a code is never valid for a
second node. Explicit `provision_wireguard: true` remains a bounded
`503 feature_unavailable` and does not consume the code while the canonical
WireGuard action adapter is unavailable.

Successful enrollment audit details are stored as structured JSON containing
only the bounded display name and device type. This avoids treating commas,
equals signs, or other user-provided characters as audit record delimiters.
Internal controller/database failures use bounded generic error envelopes and
do not return SQL, provider diagnostics, or credential material.
