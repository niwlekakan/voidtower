[Unit]
Description=VoidTower managed Linux agent
Documentation=https://github.com/elwla/voidtower/blob/dev/docs/agent/linux-agent-service.md
After=network-online.target
Wants=network-online.target

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

On a supported qualification host, record the exact output of these bounded checks before promoting the evidence:

1. `systemd-analyze verify packaging/systemd/voidtower-agent.service`
2. Install the binary and unit, create the `voidtower` user/group and `/var/lib/voidtower/agent`, then verify the state file remains owner-only (`0600`) and the state directory is not group/world writable. Before upload, provision or adopt the canonical host resource and bind it to the enrolled node; enrollment alone does not create that CMDB projection.
3. `systemctl enable --now voidtower-agent.service` and `systemctl is-active voidtower-agent.service`; inspect `systemctl status` and journal output, redact tokens, credentials, provider diagnostics, and other secrets from every captured artifact.
4. Confirm the process executes the fixed `/usr/bin/lsblk` command, uploads through the enrolled node path, and creates no inbound listener.
5. Stop the controller, observe bounded heartbeat/inventory backoff and reuse of the same pending snapshot after a failed or ambiguous upload during the running process, restore the controller, and verify recovery without duplicate snapshots. Treat process restart durability as a separate upgrade/restart check.
6. Exercise a graceful service restart, binary upgrade, and rollback while preserving compatible state; record `systemctl restart`, active status, state-file permissions, and upload recovery evidence.

Until those checks run on a named supported host, the highest valid C3-03 label is `unit-verified`; service installation, controller outage/restart recovery, upgrade, rollback, and release support remain `blocked`.
