# Managed-node enrollment and heartbeat contract

## Enrollment

`POST /api/nodes/enroll` accepts a one-time pairing code, a display name, an optional device type (`phone`, `tablet`, `pi`, or `other`), and the `agent_capable` and `provision_wireguard` flags. Pairing codes are never returned by storage reads; the database stores only their SHA-256 hash and codes expire after 15 minutes.

The request is rejected with the bounded `bad_request` envelope when the pairing code is empty or over 512 bytes, the display name is empty after trimming, over 128 bytes, or contains control characters, or the device type is outside the supported enum. The JSON body is capped at 64 KiB and an oversized body returns the stable `413 payload_too_large` envelope. These checks occur before pairing-code lookup or claim. Explicit WireGuard provisioning remains `503 feature_unavailable` until a canonical operation adapter exists and does not consume the pairing code.

A successful response contains the node UUID and a node-bound bearer token for heartbeat and inventory only. Enrollment does not create or adopt a CMDB host resource. Before inventory upload, an administrator must provision or adopt exactly one canonical `sys/host` projection and bind `resources.node_id` to the enrolled node.

The pairing-code claim and node row are committed in one database transaction. If owner resolution or node persistence fails, the transaction rolls back and the pairing code remains retryable; no partial node is created. After a successful commit, the enrollment audit details are structured JSON containing the bounded display name and device type. Node-deletion audit details use the same structured representation, so commas, equals signs, quotes, and other display-name characters are not ambiguous delimiters.

## Heartbeat

`POST /api/nodes/:node_id/heartbeat` requires the node bearer token, approved status, and `agent_capable=true`. The `Bearer` authentication scheme is matched case-insensitively (`Bearer`, `bearer`, and other casing are equivalent); the credential value is trimmed and then bounded. Authentication is performed before JSON parsing, so malformed unauthenticated input receives `401 unauthorized` rather than a parser diagnostic. Authenticated malformed JSON receives `400 bad_request` with `invalid heartbeat`.

Heartbeat telemetry is bounded: `battery`, when present, must be finite and between 0 and 100 inclusive; `storage_free_bytes`, when present, must be non-negative. Invalid telemetry receives `400 bad_request` and does not update `last_seen` or `last_telemetry`. A valid heartbeat returns `{ "ok": true }`.

## Security and qualification boundary

Node tokens are scoped to the URL node ID and are not generic API tokens. Empty or over-512-byte bearer values are rejected as `401 unauthorized` before database matching. Inventory bodies are authenticated before the 4 MiB application limit is evaluated; oversized authenticated bodies return `413 payload_too_large`. Missing, wrong, revoked, or non-agent node credentials return `401 unauthorized` without parsing inventory JSON.

Authentication for heartbeat and inventory is enforced in route middleware before the bounded body extractors run; the `Bearer` scheme comparison is case-insensitive while token bytes remain trimmed and bounded. Enrollment has its own bounded JSON extractor and stable rejection mapping. Snapshot IDs are canonicalized by trimming surrounding whitespace before identity fingerprinting, so equivalent wire representations replay safely. These contracts are covered by real Axum-router/database tests. They do not establish systemd installation, real `/usr/bin/lsblk` collection, controller outage recovery, upgrade, rollback, or release support; those C3-03 checks remain blocked until a named supported Linux host is available.

## CLI credential input

For `voidtower agent enroll`, prefer `--pairing-code-stdin` so the one-time code is read from standard input and is not placed in process arguments or shell history. Supply one UTF-8 line terminated by a newline; CRLF is accepted, the code is bounded to 512 bytes, and empty, invalid-UTF-8, or oversized input fails without echoing the credential. Only the first line is consumed; trailing stdin is not interpreted as pairing-code data.

The legacy `--pairing-code VALUE` option remains available for compatibility and its Clap debug representation is redacted, but the value can still be observed in process arguments while enrollment runs. Use stdin for supported operational procedures until that compatibility option is removed in a future breaking CLI change.
