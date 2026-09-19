[Unit]
Description=VoidTower managed Linux agent
Documentation=https://github.com/elwla/voidtower/blob/dev/docs/agent/linux-agent-service.md
After=network-online.target
Wants=network-online.target
ConditionPathExists=/var/lib/voidtower/agent/state.json

[Service]
Type=simple
User=voidtower
Group=voidtower
ExecStart=/opt/voidtower/voidtower --agent --agent-state=/var/lib/voidtower/agent/state.json
Restart=on-failure
RestartSec=5
TimeoutStopSec=15
KillSignal=SIGTERM
StandardOutput=journal
StandardError=journal
SyslogIdentifier=voidtower-agent
Environment=PATH=/usr/sbin:/usr/bin:/sbin:/bin
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictRealtime=true
RestrictSUIDSGID=true
LockPersonality=true
CapabilityBoundingSet=
AmbientCapabilities=
ReadWritePaths=/var/lib/voidtower/agent

[Install]
WantedBy=multi-user.target

The unit intentionally does not use `PrivateDevices=true`: physical-disk collection requires host `/dev` visibility. The command path is fixed to `/usr/bin/lsblk`, and the unit also supplies a fixed system PATH for other platform utilities.

Qualification status

The service package is implemented and unit-verified, but it is not runtime- or release-qualified by this checkout. Runtime qualification requires a supported Linux host with systemd and `/run/systemd/private`; the hardened development sandbox does not provide either (`systemctl` is unavailable). Do not infer service support from the unit file or collector tests.

## Release archive and installer behavior

Linux release archives contain the backend binary, built `frontend/` assets, and both systemd units under `packaging/systemd/`. The release workflow and `scripts/build-release.sh` package these files together so the agent unit is not lost when a binary release is installed.

The installer copies release frontend assets and the agent unit into the installation directory, creates `${VT_DATA_DIR}/agent` with mode `0700`, and generates units with the selected install, data, and service-user paths. It enables both `voidtower.service` and `voidtower-agent.service` when systemd is available. The agent unit has a `ConditionPathExists` guard and a controller-only installation does not start the agent until `${VT_DATA_DIR}/agent/state.json` exists; after node enrollment writes that owner-only state file, start it with `systemctl start voidtower-agent.service`. Update, repair, and uninstall stop/refresh/remove the agent unit with the controller lifecycle. The agent unit deliberately omits `PrivateDevices=true` because bounded physical-disk collection requires host `/dev` visibility.

The installer verifies the selected release archive against the release `SHA256SUMS` manifest before extraction, rejects unsafe paths and non-regular archive members, and extracts without preserving archive ownership or permissions. The same archive validation is applied to source-build and catalog tarballs. If the release checksum manifest is unavailable or does not contain the requested archive, installation fails closed rather than silently building an unrelated branch snapshot.

The checked-in `packaging/systemd/voidtower.service` is suitable for the default paths and uses `VOIDTOWER_*` environment settings rather than unsupported command-line data/config flags. The installer still renders the service unit so custom `--install-dir` and `--data-dir` values remain explicit.

Release archives are currently published for `x86_64` and `aarch64` Linux targets. The installer rejects other host architectures rather than attempting to install an unpublished archive. `--offline` performs no package-manager, source, catalog, model, or MCP pre-cache network operation; it requires local `cargo`, `npm`, source/assets, and any requested optional runtime dependencies, and uses Cargo/npm offline modes. A leading `v` is accepted on `--version` and normalized before archive lookup; an explicit offline version additionally requires a local checkout exactly tagged `v<version>`. Release checksum entries are unique per archive and hexadecimal case is normalized before comparison. Reset stops the controller and agent before wiping selected state, then restarts them in controller-first order when systemd is available.

On a supported qualification host, record the exact output of these bounded checks before promoting the evidence:

1. `systemd-analyze verify packaging/systemd/voidtower-agent.service`
2. Install the binary and unit, create the `voidtower` user/group and `/var/lib/voidtower/agent`, then verify the state file remains owner-only (`0600`) and the state directory is not group/world writable. Before upload, provision or adopt the canonical host resource and bind it to the enrolled node; enrollment alone does not create that CMDB projection.
3. `systemctl enable --now voidtower-agent.service` and `systemctl is-active voidtower-agent.service`; inspect `systemctl status` and journal output, redact tokens, credentials, provider diagnostics, and other secrets from every captured artifact.
4. Confirm the process executes the fixed `/usr/bin/lsblk` command, uploads through the enrolled node path, and creates no inbound listener.
5. Stop the controller, observe bounded heartbeat/inventory backoff and reuse of the same pending snapshot after a failed or ambiguous upload during the running process, restore the controller, and verify recovery without duplicate snapshots. Treat process restart durability as a separate upgrade/restart check.
6. Exercise a graceful service restart, binary upgrade, and rollback while preserving compatible state; record `systemctl restart`, active status, state-file permissions, and upload recovery evidence.

Agent response contracts are fail-closed: heartbeat accepts only a successful JSON response with `ok: true`, and inventory upload accepts the versioned `InventorySnapshotResultV1` fields (`snapshot_id`, `replayed`, `linked`, `registered`, `review_required`, and `missing`). The controller also authenticates node credentials before parsing heartbeat or inventory JSON; heartbeat telemetry rejects non-finite/out-of-range battery values and negative free-storage values. Collector stdout and stderr are drained concurrently, retained only within their configured bounds plus one overflow sentinel, and an overflow skips the snapshot as a bounded collection failure. A non-zero `/usr/bin/lsblk` exit, non-UTF-8 output, stderr overflow, empty output, malformed JSON, or timeout is also a bounded collection failure; timed-out command processes are killed before the agent retries. A successful HTTP status with malformed or incomplete JSON is treated as a failed operation and remains eligible for retry. Persisted agent state is validated before either supervision loop starts; invalid HTTPS, token, certificate/configuration, or schedule values stop the agent without opening requests. On Unix, state and pending-sidecar recovery also requires every existing parent directory to be non-symlinked, directory-only, not non-sticky group/other writable, and the immediate parent owner to match the protected file owner. The agent and controller share the 512-byte maximum node-token contract; exact-limit values are accepted and limit-plus-one values fail closed at both seams. On supported Linux agents, a collected inventory snapshot is first written atomically to the owner-only sidecar `.state.json.pending.json` (bounded to 256 KiB) before upload. The sidecar envelope contains schema version 1 and the enrolled node UUID as well as the snapshot; an absent sidecar is the normal first-run condition, while a present but missing, malformed, old unbound, or different-node binding fails closed rather than risking upload under the wrong identity. The sidecar is reused after process restart and removed only after a successful typed upload response; a collection failure never creates or replaces it. The pending sidecar intentionally fails closed on Windows until a supported Windows ACL implementation exists; no Windows support claim is made.

If a custom CA file is supplied during enrollment, it must be a regular, non-symlinked
UTF-8 PEM file no larger than 64 KiB with owner-only `0600` permissions. The agent
rejects group/world-readable CA files before constructing the HTTP client.

Until those checks run on a named supported host, the highest valid C3-03 label is `unit-verified`; service installation, controller outage/restart recovery, upgrade, rollback, and release support remain `blocked`.
