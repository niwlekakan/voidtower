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
