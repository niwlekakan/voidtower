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

The service package, the supervision loops, protected state, and the binary upgrade/rollback path are implemented and unit-verified. They are not runtime- or release-qualified by this checkout. Runtime qualification requires a supported Linux host where the unit can be installed at system scope; the development sandbox provides systemd 257 and a working `systemctl --user` bus but no root (`sudo -n true` fails), so a system-scope install cannot be performed there. Do not infer service support from the unit file or from collector, transport, or rollback tests.

## Upgrade and rollback

`--update` and `--repair` stop the services and then, before the new artifact is installed, copy the installed binary to `${VT_INSTALL_DIR}/voidtower.previous` and write an owner-only `0600` manifest `${VT_INSTALL_DIR}/voidtower.previous.manifest` containing `schema=1`, the binary SHA-256 digest, its byte size, the version it reported, and the retention timestamp. If retention fails the installer warns and continues; that warning is the record that this run has no rollback artifact. A fresh install retains nothing.

`sudo bash scripts/install.sh --rollback` restores the retained artifact and refuses when any of these hold:

- the install directory is missing, is group/world writable, or the operator's path is not already its own canonical absolute form — a relative path, a trailing slash, a `.`/`..` component, or any symlinked component is refused, because bash resolves a trailing slash before `-d`/`-L` and a resolved-elsewhere prefix would be checked but not written;
- the install prefix is not owned by root or the invoking user, or an ancestor above it is group/world writable without the sticky bit (a sticky ancestor such as `/tmp` is accepted, because only an entry's owner can remove it there);
- the retained artifact or its manifest is missing or is a symlink;
- the manifest does not declare `schema=1`, its `binary_size` is not a decimal integer, or that size does not match the retained file;
- the retained artifact no longer matches the recorded SHA-256 digest;
- the retained binary cannot read the current agent state file;
- the retained binary cannot be probed at all.

The state check runs `<previous> agent check-state --help` and then `<previous> agent check-state --state-path <state.json>`, an offline command that validates the protected state file exactly as the runtime does and prints the node UUID, the controller authority, the schedule, and whether a CA certificate or WireGuard configuration is present. It prints no node token, CA bytes, or WireGuard material, and it never contacts the controller. A failed probe is treated as "cannot verify", not as "predates the subcommand": a retained binary that predates `check-state`, cannot execute, or exits non-zero for any other reason makes the installer refuse, because a skip would fail open. `--allow-incompatible-state` is the explicit emergency override for either refusal; it is accepted only together with `--rollback` and is bound to that flag, never read from the environment, so `sudo -E` cannot enable it implicitly. The manifest's recorded version is sanitized to printable characters before it is printed, and the manifest is parsed field by field and never sourced.

`--rollback --dry-run` performs the same verification, prints what would be restored, and changes nothing. On success the restored artifact's digest is re-verified and the restored file is made executable; if either step fails, the artifact that failed verification is kept aside as `voidtower.failed-<UTC timestamp with nanoseconds>-<pid>` and the binary that was in place before the rollback is moved back into place, then the installer exits non-zero, so a failed restore never leaves an empty or unusable install path. The replaced binary is kept as `voidtower.replaced-<UTC timestamp with nanoseconds>-<pid>` for post-mortem, so two rollbacks in the same second cannot overwrite each other, and the manifest is removed so a second rollback cannot replay a stale artifact. The installer restarts the controller unit when systemd is available and restarts the agent unit only when `${VT_DATA_DIR}/agent/state.json` exists; otherwise it prints the exact commands to run.

## Behavioural coverage and its test doubles

`scripts/test_agent_rollback.py` sources the real installer and drives the retention, verification, and restore functions against a throwaway prefix, so no service is started and no real prefix is rewritten. It has 36 tests: 33 are behavioural (32 execute on an unprivileged host; `test_rollback_refuses_a_prefix_owned_by_another_user` needs root to `chown` a foreign prefix, is skipped here, and is **not** counted as evidence — the ownership branch stays `blocked` on root), and 3 are contract checks rather than behavioural evidence (`bash -n`, the CI-workflow step, and the `agent-rollback-contracts` release-gate entry). Two doubles are used and are disclosed here as plan section 2.1 requires: the "retained binary" is a shell stub that mimics `voidtower --version` and `agent check-state` (a rename of the real flags would be caught only by the Rust tests, not by this stub), and the restore-failure test prepends a `sha256sum` shim to `PATH` to force a post-restore digest mismatch. The suite is registered as the `agent-rollback-contracts` gate in `scripts/release-gates.json` and also runs in CI in the repository-hygiene job, so the local and CI gate definitions agree.

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
6. Exercise a graceful service restart, binary upgrade, and rollback while preserving compatible state: run `--update`, confirm `${VT_INSTALL_DIR}/voidtower.previous` and its `0600` manifest exist, run `--rollback`, and record `systemctl restart`, active status, `agent check-state` output, state-file permissions, and upload recovery evidence.

Agent response contracts are fail-closed: heartbeat accepts only a successful JSON response with `ok: true`, and inventory upload accepts a versioned `InventoryUploadEnvelopeV1` with `schema_version: 1` and nested `InventorySnapshotResultV1` fields (`snapshot_id`, `replayed`, `linked`, `registered`, `review_required`, and `missing`). The controller also authenticates node credentials before parsing heartbeat or inventory JSON; heartbeat telemetry rejects non-finite/out-of-range battery values and negative free-storage values. Collector stdout and stderr are drained concurrently, retained only within their configured bounds plus one overflow sentinel, and an overflow skips the snapshot as a bounded collection failure. A non-zero `/usr/bin/lsblk` exit, non-UTF-8 output, stderr overflow, empty output, malformed JSON, or timeout is also a bounded collection failure; timed-out command processes are killed before the agent retries. A successful HTTP status with malformed, incomplete, or unsupported-version JSON is treated as a failed operation and remains eligible for retry. Persisted agent state is validated before either supervision loop starts; invalid HTTPS, token, certificate/configuration, or schedule values stop the agent without opening requests. On Unix, state and pending-sidecar recovery also requires every existing parent directory to be non-symlinked, directory-only, not non-sticky group/other writable, and the immediate parent owner to match the protected file owner. The agent and controller share the 512-byte maximum node-token contract; exact-limit values are accepted and limit-plus-one values fail closed at both seams. On supported Linux agents, a collected inventory snapshot is first written atomically to the owner-only sidecar `.state.json.pending.json` (bounded to 256 KiB) before upload. The sidecar envelope contains schema version 1 and the enrolled node UUID as well as the snapshot; an absent sidecar is the normal first-run condition, while a present but missing, malformed, old unbound, or different-node binding fails closed rather than being uploaded. The sidecar is reused after process restart and removed only after a successful typed upload response; a collection failure never creates or replaces it. The pending sidecar intentionally fails closed on Windows until a supported Windows ACL implementation exists; no Windows support claim is made.

If a custom CA file is supplied during enrollment, it must be a regular, non-symlinked
UTF-8 PEM file no larger than 64 KiB with owner-only `0600` permissions. The agent
rejects group/world-readable CA files before constructing the HTTP client. The CA path
is opened with nonblocking semantics on Unix, so FIFOs, sockets, devices, and other
non-regular paths fail promptly instead of hanging enrollment while waiting for a
writer.

Until those checks run on a named supported host, the highest valid C3-03 label is `unit-verified`; system-scope service installation, controller outage/restart recovery against a real controller, and release support remain `blocked`. The upgrade/rollback artifact path is implemented and exercised at function level; its `systemctl`-driven restart evidence is still open for the same reason.
