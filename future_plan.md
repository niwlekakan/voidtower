# VoidTower Future Plan

This document collects recommended future-facing features for **VoidTower**, a self-hosted Linux infrastructure command tower. The goal is to make VoidTower more than a dashboard: it should become a safe, extensible, automation-aware infrastructure control plane that works well with human operators and AI agents such as Odysseus.

## Guiding principles

VoidTower should remain local-first, self-hostable, open source, and operator-focused. Every advanced feature should preserve these principles:

- No telemetry, tracking, license server, or cloud dependency.
- Dangerous actions require clear confirmation, policy approval, or dry-run review.
- AI-triggered actions must be scoped, audited, and reversible where possible.
- Operators should always understand what changed, why it changed, and how to roll it back.
- The product should be useful on one Linux host and powerful across many.

---

## 1. Plugin system

VoidTower should include a plugin system early so the core application does not become a monolith where every feature has to live forever.

Plugins should be able to register:

- Dashboard widgets.
- API routes.
- UI panels.
- Automation triggers.
- Automation actions.
- Alert providers.
- Backup providers.
- App Vault templates.
- Health checks.
- Security scanners.
- Odysseus/MCP tools.
- Infrastructure backends.

Plugin requirements:

- Plugins must declare required permissions.
- Plugins must declare required system capabilities.
- Plugins must declare whether they need elevated privileges.
- Plugins must be disabled by default until approved by an admin.
- Plugins must have a clear install, enable, disable, and remove lifecycle.
- Plugin actions must be audit logged.
- Plugins must not be able to silently bypass RBAC, policy, or confirmation rules.

Suggested prompt addition:

```text
VoidTower must include a plugin system. Plugins can register API routes, UI panels, automation actions, alert providers, App Vault templates, health checks, and Odysseus/MCP tools. Plugins must declare permissions, required system capabilities, and whether they need elevated privileges. Plugins must be disabled by default until approved by an admin.
```

---

## 2. Capability detection page

VoidTower should clearly show what each host can and cannot do before presenting advanced controls.

Create a **Capabilities** page showing:

- Docker availability.
- Docker Compose availability.
- systemd availability.
- KVM availability.
- libvirt availability.
- LXC availability.
- ZFS availability.
- Btrfs availability.
- NFS availability.
- SMB/CIFS availability.
- SSHFS availability.
- Restic availability.
- Firewall backend: UFW, firewalld, nftables, iptables, or unavailable.
- GPU detection.
- Package manager detection.
- Virtualization support enabled in BIOS/UEFI.
- Required permissions status.
- Whether the daemon is running with enough privileges for each feature.

Each capability should show:

- Detected state.
- Required dependency.
- Why it matters.
- How to enable it.
- Whether VoidTower can install/configure it safely.

This prevents confusing hidden features and helps operators understand why a button or page is unavailable.

---

## 3. Diagnostics and doctor mode

VoidTower should include both a CLI and web diagnostics system.

CLI commands:

```bash
voidtower doctor
voidtower --doctor
voidtower doctor --json
```

UI location:

```text
Settings → Diagnostics
```

Doctor checks should include:

- Service status.
- Config validity.
- File permissions.
- Port conflicts.
- Database migrations.
- Frontend asset availability.
- TLS state.
- Agent connectivity.
- Docker socket access.
- systemd integration.
- Required dependencies.
- Disk space.
- Failed App Vault deployments.
- Failed backups.
- Odysseus integration status.
- MCP server status.
- Webhook signing configuration.
- Audit log writeability.

Output formats:

- Human-readable terminal output.
- JSON output for automation.
- UI report with severity groups.

Suggested prompt addition:

```text
VoidTower must include a diagnostics system available through `voidtower --doctor`, `voidtower doctor`, and Settings → Diagnostics. It must produce human-readable output and JSON output for automation.
```

---

## 4. Disaster recovery mode

VoidTower should assume that operators may eventually break access, lose credentials, corrupt config, or need to restore a node after a failed change.

Add disaster recovery features:

- Full instance backup.
- Full instance restore.
- Config export.
- Config import.
- Emergency admin reset.
- Emergency disable Odysseus access.
- Emergency disable MCP server.
- Emergency disable all automations.
- Emergency disable webhooks.
- Cluster recovery.
- Node rejoin/reset.
- Config rollback.
- Installer rollback logs.
- Recovery bundle generation for support/debugging.

Suggested CLI commands:

```bash
voidtower backup-instance
voidtower restore-instance <archive>
voidtower emergency-disable-ai
voidtower emergency-disable-automations
voidtower emergency-disable-webhooks
voidtower reset-admin
voidtower recover-cluster
voidtower export-config
voidtower import-config <file>
```

Recovery mode should never require the web UI to be working.

---

## 5. Change planning and dry-run system

Before performing dangerous or system-changing actions, VoidTower should be able to show a clear change plan.

Dry-run previews should apply to:

- Firewall changes.
- Reverse proxy changes.
- App Vault deployments.
- Package installs.
- Package removals.
- Service restarts.
- Backup restores.
- VM deletion.
- Container deletion.
- Volume deletion.
- Node removal.
- Cluster secret rotation.
- Automation runs.
- Odysseus-triggered actions.

A change plan should show:

- Commands that will run.
- Files that will be created, changed, or deleted.
- Services that will restart.
- Ports that will open or close.
- Containers that will be created or removed.
- Volumes that will be mounted or deleted.
- Whether rollback is available.
- Estimated risk level.
- Whether manual approval is required.

Suggested prompt addition:

```text
VoidTower must support dry-run previews for dangerous or system-changing actions. The UI should show a change plan before execution, including files touched, commands run, services restarted, ports opened, containers created, volumes mounted, and rollback availability.
```

This is especially important when AI agents are allowed to request infrastructure changes.

---

## 6. Policy engine for automation and AI

VoidTower should include a policy engine that controls what humans, automations, plugins, and AI agents are allowed to do.

Policy examples:

- AI can restart non-critical services automatically.
- AI can never touch databases without approval.
- Backups can run automatically.
- Firewall changes always require manual approval.
- Shell commands require approval unless allowlisted.
- Only Owner can enable MCP tools.
- App deployments are allowed only on specific nodes.
- Production-tagged resources require stricter approval.
- Actions outside maintenance windows require approval.

Policy dimensions:

- Actor: user, automation, plugin, Odysseus, MCP client, API token.
- Action type.
- Resource type.
- Resource tag.
- Node.
- Time window.
- Risk level.
- Required approval.
- Expiry.

Suggested prompt addition:

```text
VoidTower must include a policy engine for automation and AI-triggered actions. Policies define who or what may perform actions, on which resources, during which time windows, and with what approval requirements.
```

---

## 7. Maintenance windows

VoidTower should support first-class maintenance windows.

Features:

- Schedule maintenance windows.
- Suppress selected alerts during maintenance.
- Show a status page maintenance banner.
- Allow selected automations only during maintenance.
- Require approval for certain actions outside maintenance.
- Notify users before and after maintenance.
- Integrate with Odysseus so an AI agent can plan, summarize, or monitor maintenance.

Maintenance window fields:

- Name.
- Start time.
- End time.
- Affected nodes/resources.
- Suppressed alert categories.
- Allowed actions.
- Notes.
- Status page message.

---

## 8. Resource tagging

Every major object in VoidTower should support tags.

Taggable resources:

- Nodes.
- Containers.
- Docker Compose apps.
- VMs.
- Services.
- Backups.
- Automations.
- Alerts.
- Status checks.
- App Vault deployments.
- API tokens.
- Odysseus tools/policies.

Example tags:

- `prod`
- `lab`
- `critical`
- `database`
- `media`
- `gpu`
- `backup-required`
- `ai-no-touch`
- `maintenance-only`
- `public-facing`

Tags should be usable for:

- Filtering.
- Dashboards.
- Alert routing.
- Backup policies.
- Automation policies.
- Odysseus policy decisions.
- Access controls.
- Maintenance windows.

---

## 9. Secrets manager

VoidTower will need to store many secrets, so it should include a local encrypted secrets manager instead of scattering credentials across config files.

Secrets may include:

- API keys.
- Webhook secrets.
- SMTP credentials.
- S3 credentials.
- Odysseus tokens.
- App deployment variables.
- Backup repository passwords.
- Database passwords.
- DNS provider credentials.
- TLS private key metadata.

Secrets manager requirements:

- Encrypt secrets at rest.
- Use a per-installation encryption key.
- Never show plaintext secrets again after save.
- Support secret rotation.
- Support scoped access.
- Audit every read/use where practical.
- Redact secrets from logs.
- Redact secrets from Odysseus context bundles.
- Export secrets only as part of encrypted instance backup.
- Provide a UI for creating, editing metadata, rotating, and deleting secrets.

Suggested UI location:

```text
Settings → Security → Secrets
```

---

## 10. Snapshot-aware operations and rollback points

Before dangerous changes, VoidTower should create rollback points where possible.

Rollback mechanisms:

- VM snapshot if backend supports it.
- Btrfs/ZFS snapshot if resource lives on supported filesystem.
- Backup Docker Compose file.
- Backup `.env` file.
- Backup systemd override file.
- Backup firewall ruleset.
- Backup reverse proxy config.
- Backup app deployment metadata.
- Checkpoint container metadata.

UI features:

- Rollback last change.
- View rollback points.
- Compare config before/after.
- Show whether rollback is full, partial, or unavailable.
- Require confirmation before rollback.

Rollback metadata:

- Resource.
- Change type.
- Created by.
- Created at.
- Associated audit event.
- Associated Odysseus action if applicable.

---

## 11. Inventory and asset database

VoidTower should maintain a lightweight infrastructure inventory, similar to a small CMDB.

Track:

- Host hardware.
- CPU model/count.
- RAM.
- Disks.
- GPU devices.
- Network interfaces.
- Serial numbers where available.
- OS versions.
- Kernel versions.
- Installed packages.
- Exposed services.
- IP addresses.
- DNS names.
- Certificates.
- Owners.
- Resource notes.
- Warranty/support metadata entered manually.

Useful questions this enables:

- What changed on this node since last week?
- Which nodes run public-facing services?
- Which servers have GPUs?
- Which services have no backups?
- Which nodes are out of date?
- Which resources are tagged `ai-no-touch`?

---

## 12. Global infrastructure timeline

VoidTower should have a global timeline of important events.

Timeline events:

- User logins.
- Failed login attempts.
- Service restarts.
- Container actions.
- VM actions.
- App deployments.
- Backups.
- Backup restores.
- Alerts.
- Config changes.
- Node joins/leaves.
- Cluster secret rotations.
- AI-triggered actions.
- Automation runs.
- Failed commands.
- Theme/settings changes.
- Plugin installation/enabling/disabling.

Timeline features:

- Filter by node.
- Filter by resource.
- Filter by user/actor.
- Filter by severity.
- Filter by action type.
- Export selected range.
- Link events to audit entries.
- Link Odysseus actions to related events.

This becomes critical during incident response.

---

## 13. Incident mode

VoidTower should support an incident workflow when something breaks.

Incident features:

- Create incident from alert.
- Attach logs.
- Attach metrics.
- Attach services/containers/VMs.
- Add notes.
- Assign owner.
- Track status.
- Show timeline.
- Publish status page update.
- Send investigation task to Odysseus.
- Export postmortem.

Incident statuses:

- Investigating.
- Identified.
- Monitoring.
- Resolved.
- Postmortem pending.

Odysseus integration:

- Draft investigation summary.
- Suggest likely cause.
- Draft remediation plan.
- Draft postmortem.
- Monitor related metrics until resolved.

---

## 14. Config drift detection

VoidTower should detect when managed resources change outside of VoidTower.

Detect drift for:

- systemd units.
- Docker Compose files.
- Container image tags.
- Firewall rules.
- Reverse proxy configs.
- Backup configs.
- Automation definitions.
- App Vault deployments.
- Node config.
- Cluster state.

Drift UI should show:

- Expected state.
- Actual state.
- Diff.
- Last known good state.
- Who/what last changed it if known.
- Reconcile button.
- Accept external change button.
- Ignore rule.

This is very useful when operators use both UI and shell.

---

## 15. Declarative state mode

VoidTower should eventually support GitOps-style declarative infrastructure state.

Example:

```yaml
nodes:
  alpha:
    tags:
      - prod
      - backup-required
    apps:
      - gitea
      - uptime-kuma
    backups:
      enabled: true
    allowed_ai_actions:
      - restart_noncritical_services
```

Benefits:

- Version-controlled infrastructure.
- Reviewable changes.
- Reproducible setup.
- Rollbacks.
- Better disaster recovery.
- Odysseus can draft changes instead of directly modifying production.

Features:

- Export current state as YAML.
- Import desired state.
- Show diff.
- Dry-run apply.
- Apply with rollback points.
- Optional Git repository sync.
- Optional pull-request-style workflow.

---

## 16. Built-in notes per resource

Operators should be able to attach notes to resources.

Resources with notes:

- Nodes.
- Containers.
- VMs.
- Services.
- Alerts.
- Automations.
- Backups.
- App Vault deployments.
- Security findings.

Notes should support:

- Markdown.
- Pinning important notes.
- Last edited metadata.
- Search.
- Tags.
- Links to runbooks.
- Optional visibility restrictions.

Example use:

> Do not restart this service during working hours. It handles family photo storage and therefore has political immunity.

---

## 17. Public API SDKs

VoidTower should generate SDKs from its OpenAPI spec.

SDKs:

- TypeScript.
- Python.
- Go.

Use cases:

- Odysseus integration.
- Plugins.
- Custom scripts.
- External monitoring.
- CI/CD pipelines.
- Homelab automation.

SDK requirements:

- Auth helpers.
- Typed request/response models.
- Examples.
- Versioning.
- Generated from OpenAPI.

---

## 18. Backup verification

Backups must be verified, not merely created.

Features:

- Scheduled restore tests.
- Checksum verification.
- Restic check.
- Backup repository health.
- Backup age warning.
- Last successful backup time.
- Last successful restore test time.
- Backup confidence score.
- Alert when backups exist but have never been restore-tested.

Dashboard card:

```text
Backup Confidence
- Last backup: 2 hours ago
- Last restore test: 3 days ago
- Repository check: healthy
- Confidence: High
```

Suggested prompt addition:

```text
VoidTower must include backup verification. A backup is not considered healthy unless it has been checked and, where configured, restore-tested.
```

---

## 19. Update management

VoidTower should include controlled update handling.

Update types:

- VoidTower application updates.
- App Vault template updates.
- Docker image updates.
- OS package updates.
- Plugin updates.
- Agent node updates.

Update requirements:

- Show available updates.
- Show changelog.
- Show risk level.
- Support dry-run.
- Create backup/rollback point before upgrade.
- Never auto-upgrade without explicit approval.
- Allow scheduled maintenance-window updates.
- Support staggered node updates.
- Detect failed upgrades.
- Roll back where possible.

---

## 20. Demo and simulation mode

VoidTower should include a demo mode for development, screenshots, testing, and onboarding.

CLI:

```bash
voidtower --demo
```

Demo mode should create fake:

- Nodes.
- Metrics.
- Alerts.
- Containers.
- VMs.
- App deployments.
- Backups.
- Status pages.
- Automation runs.
- Audit events.
- Odysseus integration events.

Demo mode must not touch the real host.

Benefits:

- Easier UI development.
- Easier automated screenshots.
- Easier documentation.
- Easier onboarding.
- Safer testing of destructive-action UX.

---

## Recommended must-have additions

The following should be treated as non-negotiable for a serious first public release:

1. Plugin system.
2. Capability detection.
3. Doctor/diagnostics mode.
4. Disaster recovery mode.
5. Dry-run/change planning.
6. Policy engine for automation and Odysseus.
7. Secrets manager.
8. Resource tags.
9. Global timeline.
10. Backup verification.

These features make VoidTower safer, more extensible, and more credible as an infrastructure control plane.

---

## Suggested acceptance criteria additions

Add these to the main project acceptance criteria:

```text
- VoidTower includes a plugin architecture with permission declarations.
- VoidTower includes a Capabilities page showing what each host supports.
- VoidTower includes `voidtower doctor` and a Diagnostics UI.
- VoidTower includes disaster recovery commands for admin reset, instance backup/restore, and emergency AI disablement.
- VoidTower supports dry-run change plans for dangerous actions.
- VoidTower includes a policy engine for human, automation, plugin, API-token, MCP, and Odysseus actions.
- VoidTower supports maintenance windows.
- VoidTower supports resource tags across major resources.
- VoidTower includes a local encrypted secrets manager.
- VoidTower creates rollback points for supported dangerous operations.
- VoidTower includes a global infrastructure timeline.
- VoidTower includes incident mode.
- VoidTower detects config drift for managed resources.
- VoidTower can export and apply declarative desired state in dry-run mode.
- VoidTower supports notes on major resources.
- VoidTower publishes generated TypeScript, Python, and Go SDKs from OpenAPI.
- VoidTower verifies backups and exposes backup confidence state.
- VoidTower includes controlled update management with changelog, dry-run, and rollback support.
- VoidTower includes demo/simulation mode that does not touch the real host.
```
