# Managed-node enrollment and heartbeat contract

## Enrollment

`POST /api/nodes/enroll` accepts a one-time pairing code, a display name, an optional device type (`phone`, `tablet`, `pi`, or `other`), and the `agent_capable` and `provision_wireguard` flags. Pairing codes are never returned by storage reads; the database stores only their SHA-256 hash and codes expire after 15 minutes.

The request is rejected with the bounded `bad_request` envelope when the pairing code is empty or over 512 bytes, the display name is empty after trimming, over 128 bytes, or contains control characters, or the device type is outside the supported enum. These checks occur before pairing-code lookup or claim. Explicit WireGuard provisioning remains `503 feature_unavailable` until a canonical operation adapter exists and does not consume the pairing code.

A successful response contains the node UUID and a node-bound bearer token for heartbeat and inventory only. Enrollment does not create or adopt a CMDB host resource. Before inventory upload, an administrator must provision or adopt exactly one canonical `sys/host` projection and bind `resources.node_id` to the enrolled node.

## Heartbeat

`POST /api/nodes/:node_id/heartbeat` requires the node bearer token, approved status, and `agent_capable=true`. Authentication is performed before JSON parsing, so malformed unauthenticated input receives `401 unauthorized` rather than a parser diagnostic. Authenticated malformed JSON receives `400 bad_request` with `invalid heartbeat`.

Heartbeat telemetry is bounded: `battery`, when present, must be finite and between 0 and 100 inclusive; `storage_free_bytes`, when present, must be non-negative. Invalid telemetry receives `400 bad_request` and does not update `last_seen` or `last_telemetry`. A valid heartbeat returns `{ "ok": true }`.

## Security and qualification boundary

Node tokens are scoped to the URL node ID and are not generic API tokens. Empty or over-512-byte bearer values are rejected as `401 unauthorized` before database matching. Inventory bodies are authenticated before the 4 MiB application limit is evaluated; oversized authenticated bodies return `413 payload_too_large`. Missing, wrong, revoked, or non-agent node credentials return `401 unauthorized` without parsing inventory JSON.

Authentication for heartbeat and inventory is enforced in route middleware before the bounded body extractors run; these contracts are covered by real Axum-router/database tests. They do not establish systemd installation, real `/usr/bin/lsblk` collection, controller outage recovery, upgrade, rollback, or release support; those C3-03 checks remain blocked until a named supported Linux host is available.
