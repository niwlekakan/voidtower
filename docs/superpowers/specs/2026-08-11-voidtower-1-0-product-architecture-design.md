# VoidTower 1.0 Product and Architecture Design

**Status:** Approved design, pending consolidated-document review

**Date:** 2026-08-11

**Release target:** VoidTower 1.0

## 1. Purpose

VoidTower 1.0 is a public, self-hosted operating and management platform that must also run the owner's real infrastructure and household. It combines three equally required product pillars:

1. A secure and dependable infrastructure control plane.
2. A branded household experience spanning mobile, media, devices, people, rooms, and family workflows.
3. AI, plugin, automation, and multi-node operation across the entire platform.

This specification replaces feature-count-driven prioritization with a foundation-first release design. All three pillars remain in 1.0, but they are delivered in dependency order so higher-level experiences do not rest on unsafe or temporary infrastructure.

This design is grounded in:

- `ROADMAP.md`, including its current-state inventory and long-term product ambitions.
- `docs/project-reality-audit-2026-08-11.md`, which verifies roadmap and documentation claims against the current repository and CI state.
- `Codex Handoff — VoidTower CMDB - Asset Registry.md`, whose asset-registry requirements are incorporated as a core platform boundary.
- The existing Rust/Axum backend, React web and Tauri clients, Expo mobile client, SQLite persistence, App Vault, Proxmox integration, policy primitives, and Odysseus integration.

## 2. Product principles

### 2.1 One platform, three pillars

Infrastructure, the household experience, and AI are not separate products. They share identity, resources, policy, jobs, events, audit history, and clients.

### 2.2 Local-first and self-hosted

The owner controls the control plane, operational data, credentials, model-provider choices, and retention. External services are optional integrations rather than mandatory license, telemetry, or identity dependencies.

### 2.3 One authoritative path for mutation

Human UI actions, mobile actions, automation, AI, plugins, and external API clients use the same typed capability and execution pipeline. No integration receives a hidden privileged path.

### 2.4 Capability-driven heterogeneity

Machines join one managed estate without needing identical hardware or software. VoidTower discovers what each node can safely do and schedules only compatible work.

### 2.5 Honest release evidence

"Code exists" and "production ready" are different states. Product claims distinguish target, source implemented, contract verified, integration verified, failure verified, hardware/provider verified, and release verified.

### 2.6 Explainable safety

Automatic placement, AI recommendations, approvals, and mutations expose their inputs, selected targets, expected effects, risks, and verified outcomes.

## 3. Scope and non-goals

### 3.1 Required for 1.0

- A recoverable authoritative control plane.
- Secure heterogeneous node enrollment and management.
- Canonical CMDB identity and discovery.
- Node-aware Docker/App Vault and VM/LXC provisioning.
- Native libvirt/KVM and full Proxmox provider workflows.
- Cluster-native automation and managed community scripts.
- Isolated, capability-scoped plugins.
- AI access to view, explain, report on, and safely manage every applicable domain.
- First-class web, desktop, iOS, Android, and tablet experiences.
- Branded household resources and selected family workflows built on the same platform model.
- Clean-install, upgrade, recovery, multi-node, provider, security, and mobile release evidence.

### 3.2 Explicit 1.0 non-goals

- Kubernetes, etcd, or a general distributed-consensus platform.
- Multiple active control planes or automatic control-plane failover.
- Pretending that heterogeneous nodes provide identical capabilities.
- Silent relocation of stateful workloads without compatible storage and recovery semantics.
- A separate household database, permission model, or automation system.
- AI bypasses, unrestricted plugin database access, or unreviewed remote `curl | shell` execution.
- Automatic HA scheduling or live migration where the underlying provider cannot guarantee it.

The single-control-plane design must leave clean persistence and protocol boundaries for future active/passive or multi-control-plane work.

## 4. System architecture

VoidTower remains an incremental Rust/Axum modular monolith for 1.0. It will not be rewritten into multiple services merely to match older greenfield documents. Clear internal module boundaries, versioned APIs, durable persistence, and an external node protocol provide the necessary separation.

### 4.1 Deployment roles

| Role | Contents | Purpose |
|---|---|---|
| Control plane | API, database, scheduler, policy, CMDB, provider coordination, and UI hosting | Authoritative estate management |
| Managed node | `vt-agent` and capability adapters | Manages one machine under control-plane authority |
| Standalone/combined | Control plane and `vt-agent` on one machine | Complete single-host installation that can add nodes later |

A combined installation uses the same agent contract as a remote node. Existing local-only collectors and operations migrate behind local adapters instead of forming a second management implementation.

The control plane may run on any release-supported host or container environment that satisfies its storage and networking requirements. It does not need to run on the most capable infrastructure node.

### 4.2 Terminology

`vt-agent` means the machine-management runtime. AI characters or activity visualizations in the user interface are called **AI actors** and never share the node-agent API or persistence model. This resolves the current ambiguity between infrastructure agents and the existing activity-overlay agent concept.

### 4.3 Major subsystem responsibilities

| Subsystem | Owns | Does not own |
|---|---|---|
| Identity and access | Users, roles, sessions, API tokens, node identities | Domain execution logic |
| CMDB / Asset Registry | Stable identity, aliases, classifications, administrative metadata, locations, relationships, lifecycle, history | Live operations or provider truth |
| Observation and discovery | Provider observations, provenance, normalization, correlation, presence, discovery inbox | User-owned administrative truth |
| Agent hub and `vt-agent` | Remote inventory, health, local adapters, authorized job execution | Global policy, approval, placement, canonical identity |
| Capability registry | Typed resource reads, actions, events, reports, risk metadata, AI exposure | Performing actions directly |
| Policy, approval, and jobs | Authorization, risk classification, plans, approvals, durable execution, retries, cancellation, audit | Provider-specific implementation |
| Operational domains | Storage, compute, containers, networking, apps, backups, media | Independent resource identities or bypass paths |
| Placement service | Eligibility, constraints, ranking, explanations | Provider execution or hidden relocation |
| Automation engine | Triggers, workflow state, typed steps, multi-node coordination | Unrestricted implicit authority |
| Plugin host | Isolated child processes, capability-scoped versioned RPC | Direct database or unrestricted host access |
| AI integration | Context, tools, reports, recommendations, action proposals, verified reporting | Separate privileged actions or invented state |
| Household experience | Household workflows using shared resources, policy, jobs, and events | A parallel control plane |
| Clients | Web, Tauri, Expo, API and CLI experiences | Authoritative state |

### 4.4 Control-plane availability model

VoidTower 1.0 uses one authoritative control plane managing many nodes.

- Nodes continue existing workloads during a control-plane outage.
- New mutations pause safely.
- Encrypted scheduled backups preserve the database, secrets, trust material, configuration, and required metadata.
- Restore to a replacement host preserves asset identities, audit history, and node trust where the trust material is restored.
- Reconnected nodes reconcile observations before new commands run.
- The product does not claim automatic control-plane HA in 1.0.

## 5. Canonical CMDB and discovery

The CMDB is VoidTower's persistent identity layer for physical, virtual, application, and household resources. Operational domains attach to CMDB assets; they do not maintain competing canonical identities.

### 5.1 Asset identity

Every asset has:

- An immutable internal UUID used for database relationships and APIs.
- A configurable human identifier such as `VT-HW-DISK-000042`.
- Retained aliases when names or identifiers change.
- A data-driven class and type.
- A lifecycle state distinct from live discovery/presence state.
- User-owned administrative fields.
- Provider observations with source and timestamp.
- Generic relationships and relationship history.

Human ID allocation is transactional. Renaming never changes the UUID and never discards old aliases.

### 5.2 Administrative truth and observed truth

Administrative and observed values remain separate. A provider may report a model, hostname, disk path, or current location without silently overwriting a user-defined name, owner, lifecycle, note, or intended location.

The UI and API expose provenance, observation age, and field precedence. Merges retain identifiers, provider history, relationships, and audit evidence.

### 5.3 Discovery pipeline

```text
local collector, vt-agent, or provider
    -> raw provider observation
    -> normalization
    -> stable-identity correlation
    -> existing asset or discovery candidate
    -> registration/update
    -> relationships, history, events, and capabilities
```

The initial discovery policy is **Review First**. Strong identifiers from explicitly trusted providers may support automatic registration later. Ambiguous matches never silently merge or create duplicate canonical assets.

### 5.4 First hardware identity path

Physical disks are the first mandatory end-to-end discovery flow. Stable identity preference is:

1. WWN.
2. NVMe UUID.
3. Serial plus model.
4. Serial alone.
5. Uncorrelated candidate requiring review.

Paths such as `/dev/sda` are observations, never identities. M.2 describes form factor and NVMe describes a protocol; the type model must not conflate them.

The decisive acceptance flow is: a disk appears in host A, becomes absent without deletion, then appears in host B and remains one asset with preserved history and an updated host relationship.

### 5.5 First CMDB vertical slice

- Configurable asset classes, types, and ID counters.
- Manual asset creation and editing.
- Aliases, locations, relationships, lifecycle, and history.
- Enrolled nodes represented as host assets.
- Versioned inventory observation contracts.
- Linux disk observations containing strong identifiers.
- Correlation, review inbox, merge handling, and provenance.
- Storage views linked to canonical asset UUIDs.
- Authorization/risk registry entries, events, timeline, and audit integration.

## 6. Managed nodes and cluster model

Every installation forms a cluster, even when it begins as one combined machine.

### 6.1 Node responsibilities

A managed node may report and manage, subject to capabilities and policy:

- Hardware, disks, filesystems, GPUs, and network interfaces.
- OS, kernel, packages, updates, and reboot requirements.
- Services, processes, logs, metrics, and health.
- Docker Engine, Compose projects, containers, images, and volumes.
- Native KVM/libvirt virtualization.
- Storage pools, bridges, and provider-specific host capabilities.
- Authorized local jobs and structured progress.
- Local workload continuity while disconnected.

Nodes do not independently decide canonical identity, workload placement, user authorization, risk, or approval.

### 6.2 Enrollment and trust

1. An owner creates a short-lived enrollment token.
2. The agent exchanges it for a persistent node identity and certificate.
3. The node opens an outbound authenticated encrypted connection.
4. The control plane correlates or creates its CMDB host asset.
5. The node reports capabilities and inventory.
6. Certificates support rotation, revocation, and recovery-aware re-enrollment.

Inbound management ports are not required. A managed node is headless by default, with a local CLI for status, diagnostics, enrollment, and emergency disconnect plus a loopback-only health endpoint.

### 6.3 Support tiers

| Node class | 1.0 intent |
|---|---|
| Linux infrastructure node | Full inventory, monitoring, services, storage, networking, Docker, libvirt/KVM, updates, and automation |
| Proxmox VE node/cluster | Full Proxmox provider workflows plus optional local-agent capabilities |
| Windows/macOS endpoint | Enrollment, inventory, telemetry, software/update state, services/processes, files, and platform-appropriate automation |
| Agentless asset | API, SSH, SNMP, Redfish, or plugin-based discovery and capabilities |

The base protocol is OS- and architecture-neutral. Release-tested distributions, architectures, and provider versions are listed explicitly; other combinations are labelled experimental rather than implied by "any machine."

## 7. Placement and workload management

### 7.1 Shared placement service

VMs, containers, App Vault deployments, AI workloads, and automations use one placement service. It evaluates:

- Required provider and capabilities.
- CPU, memory, disk, and GPU capacity.
- Architecture and operating system.
- Storage locality and portability.
- Network, bridge, VLAN, and device requirements.
- Labels, affinity, and anti-affinity.
- Health, connectivity, and maintenance state.
- Ownership, role, and user quotas.
- Backup and recovery requirements.

Users may select a specific eligible node or request automatic placement. Every automatic choice shows its constraints, ranking factors, selected node, capacity impact, warnings, and approval requirements.

### 7.2 VM and LXC provisioning

The common creation flow captures:

- Template, ISO, cloud image, or container template.
- CPU, RAM, disk, network, and optional devices.
- Owner, tags, purpose, and quotas.
- Preferred or required node.
- Storage pool and backup/snapshot policy.
- Provider-specific advanced settings.

VoidTower plans the change, obtains required approval, runs a durable job through the selected provider, verifies the resulting state, creates or links CMDB assets and relationships, and reports the outcome.

Native node virtualization targets KVM/libvirt. Existing Proxmox environments participate through the Proxmox provider. Both share the common resource and job model without hiding advanced provider-specific features.

### 7.3 Docker and App Vault on nodes

App deployment performs eligibility and placement checks, shows generated Compose configuration, ports, volumes, devices, secrets, and risks, then executes on the selected node.

VoidTower 1.0 uses Docker Compose per node, not Swarm or Kubernetes. Deployment storage is explicitly classified:

- **Node-local:** tied to one host.
- **Shared:** backed by an identified shared-storage asset.
- **Portable:** recoverable or redeployable through a defined backup workflow.

A node failure marks affected applications unavailable. VoidTower does not silently start a second stateful copy without a safe storage and recovery plan.

## 8. Full Proxmox integration

An existing Proxmox cluster enrolls through a scoped API token. Clusters, nodes, guests, storage, networks, backups, and relevant tasks become CMDB assets or relationships. Installing `vt-agent` on Proxmox hosts is optional and adds capabilities unavailable through the Proxmox API.

The 1.0 provider targets:

- Cluster, quorum, node, subscription, maintenance, and task health.
- QEMU VM and LXC creation, cloning, templates, deletion, and lifecycle.
- Cloud-init, ISO, and container-template management.
- CPU, memory, disk, network-device, and provider-specific configuration.
- Console access, snapshots, rollback, migration, and replication.
- Backup jobs, restore workflows, and Proxmox Backup Server integration.
- Storage pools, content, ZFS, LVM, directory storage, and Ceph visibility.
- Bridges, bonds, VLANs, SDN, and firewall configuration.
- PCI, GPU, USB, and physical-disk passthrough.
- Resource pools, tags, ownership, placement, and quotas.
- HA visibility and provider-supported operations.
- Full task progress, errors, verification, and audit history.

Dangerous cluster, storage, firewall, passthrough, deletion, and restore actions always use explicit plans and risk-aware approvals.

## 9. Automation and community content

### 9.1 Cluster-native automation

Automations support:

- Schedule, event, webhook, alert, and state-change triggers.
- Single-node and multi-node workflows.
- Static targets and dynamic CMDB queries.
- Conditions, branches, approvals, maintenance windows, and reusable templates.
- Typed Docker, Proxmox, libvirt, storage, network, service, household, and plugin actions.
- Durable state, retries, timeouts, cancellation, and explicit compensation.
- Per-step authorization, risk, progress, output, and audit.
- Versioned workflow definitions and run history.

Raw shell remains an explicit high-risk Owner/Admin capability. Ordinary automation uses typed capabilities so actions can be validated, previewed, authorized, and audited.

### 9.2 Community scripts

Community scripts, including Proxmox helper workflows, use a managed catalog instead of an unrestricted remote-execution interface. Each entry records:

- Source repository, immutable revision, checksum, and maintainer.
- Supported platforms and compatibility constraints.
- Required privileges, network access, filesystem changes, and secrets.
- Inputs, outputs, resources created, risk, and verification checks.
- Upgrade, uninstall, and rollback behavior where available.

Execution performs source verification, compatibility checks, a generated change plan, policy and approval, a durable node job, validation, CMDB reconciliation, and audit. Catalog synchronization is opt-in, and an update never changes a previously approved revision silently.

## 10. AI as a universal platform interface

AI integration is a cross-cutting completion requirement, not a late feature. Every applicable resource and domain declares an AI contract when it is implemented.

### 10.1 AI contract

| Contract | Purpose |
|---|---|
| Resource schema | Defines what the resource is and which fields may enter context |
| Read capabilities | State, metrics, configuration, relationships, and history |
| Action capabilities | Typed operations AI may propose or invoke |
| Event schema | Changes and alerts AI may monitor |
| Report schema | Structured summaries, trends, evidence, and export options |
| Risk metadata | Authorization, approval, and autonomy requirements |
| Redaction rules | Data excluded or transformed before model access |
| Provenance | Source and observation time for reported facts |

The capability registry generates or supplies versioned MCP and other AI-facing schemas. Plugins and providers gain AI integration through the same contracts.

### 10.2 AI management flow

```text
user or event request
    -> intent and permitted context
    -> authoritative state queries
    -> typed action proposal
    -> policy and risk evaluation
    -> preview or approval
    -> durable job execution
    -> outcome verification
    -> evidence-linked report
    -> audit and timeline
```

The autonomy ladder is:

1. Read and explain.
2. Recommend.
3. Prepare a change plan.
4. Execute permitted low-risk action classes.
5. Request approval for elevated actions.
6. Require explicit human approval for irreversible actions.

Proactive observation and reporting can be enabled by user and domain. Proactive mutation is separately governed by policy.

### 10.3 Universal AI experience

Applicable web, desktop, and mobile surfaces offer contextual actions such as:

- Ask about this resource.
- Explain health, configuration, relationships, or history.
- Summarize recent changes.
- Investigate an alert or failure.
- Recommend and compare next actions.
- Generate a change plan.
- Generate or schedule a report.
- Open supporting assets and events.

Answers cite authoritative VoidTower assets, observations, jobs, and audit events. The AI may narrate evidence but may not invent measurements or report a mutation as successful before verification.

### 10.4 Reports

Initial report families cover:

- Estate and CMDB inventory.
- Node health, performance, and capacity.
- VM, container, application, and provider availability.
- Storage health and capacity trends.
- Backup coverage and restore confidence.
- Security posture, access, policy, and audit activity.
- Configuration drift and pending updates.
- Proxmox cluster health and failures.
- Automation effectiveness and failure patterns.
- Household device, media, and service status.

Reports may be interactive, scheduled, or event-triggered and retain links to their structured evidence.

### 10.5 Provider independence and privacy

Odysseus is the deeply integrated local-first experience, while local and optional external providers can implement the same contracts. Users may have provider, model, budget, and privacy policies with an owner-defined fallback.

Context is minimized and redacted. Secrets, precise location, finance, household members, files, and terminal output receive stricter domain policies. Switching providers does not change the user's underlying permissions.

## 11. Household experience layer and branding

The household experience is a layer over the common platform rather than a parallel implementation. Its people, rooms, devices, services, media systems, and manual household inventory are CMDB assets or relationships governed by shared identity, policy, jobs, events, and audit.

`HomeOS` is only a temporary planning label and must not ship as the public feature or product name. Before household UI and marketing copy are finalized, VoidTower requires a dedicated naming and identity exercise that:

- Produces a distinctive name connected to the VoidTower universe without sounding like a generic operating-system category.
- Balances the technical character of VoidTower with a warmer, calm, trusted household identity.
- Defines how the name appears in navigation, onboarding, mobile, documentation, and voice interactions.
- Establishes a compact visual and verbal identity that still belongs to the shared VoidTower design system.
- Checks naming, domain, package, and relevant trademark conflicts before public adoption.
- Tests the name with infrastructure-focused and household-focused users rather than selecting it only from internal preference.

The 1.0 household track includes:

- Household member profiles, roles/groups, onboarding, and per-user experiences.
- Rooms, locations, devices, ownership, and manual inventory.
- Selected media, Home Assistant, calendar, notification, and family workflows.
- Mobile-first household controls and status.
- Explicit privacy policies for sensitive family data.
- The same AI read, report, action, approval, and evidence contracts as infrastructure domains.

## 12. Smartphone and tablet clients

The Expo application becomes a first-class iOS, Android, and tablet client. Its current scaffolding is a starting point, not proof of 1.0 readiness. Capability parity is expressed through mobile-appropriate flows rather than copying every desktop table and editor.

### 12.1 Primary mobile navigation

1. **Home:** estate and household status.
2. **Activity:** alerts, incidents, jobs, approvals, and notifications.
3. **Assets:** nodes, VMs, applications, devices, relationships, and scanning.
4. **Automations:** runs, status, safe triggers, and pause/resume.
5. **AI:** questions, investigation, reports, voice, and management.

Advanced and infrequent settings use contextual screens. Primary actions remain thumb-accessible, tap targets are at least 44 points, and technical density is progressively disclosed.

### 12.2 Required capabilities

- Secure instance pairing through QR code, link, or short code.
- Multiple VoidTower instance profiles.
- Platform secure storage and biometric app unlock.
- Role-aware infrastructure and household dashboards.
- Cluster, node, VM, container, App Vault, alert, incident, and job views.
- Native approval and denial flows with change-plan details.
- CMDB search, relationship history, and camera-based QR/barcode workflows.
- Household rooms, devices, media, and family controls under the selected brand.
- Automation monitoring and safe triggers.
- AI questions, voice input, investigation, recommendations, and reports.
- Deep links from notifications to the exact resource, job, or approval.
- Accessible phone and tablet layouts.

Raw terminal and high-risk infrastructure configuration are deliberately protected advanced flows, not prominent mobile shortcuts.

### 12.3 Smartphone as an asset

A phone may enroll as an endpoint node and CMDB asset. With explicit platform permissions it can report device identity, OS/app version, battery, storage, connectivity, notification reachability, and optional location/presence. Mobile sandbox constraints mean it is not equivalent to a Linux infrastructure node.

Location, microphone, camera, photos, and local-network discovery are opt-in with clear retention and AI-sharing controls.

### 12.4 Offline and notification behavior

Mobile clients cache recent read-only state and show its age. Destructive actions and approvals are never silently queued offline; they require fresh state and authorization after reconnecting.

Notifications cover critical alerts, approvals, job outcomes, node outages/recovery, backup failures, automation events, and scheduled AI reports. Push payloads contain minimal sensitive data and retrieve full details only after authenticated launch.

## 13. Durable state, failures, and errors

### 13.1 State semantics

VoidTower explicitly distinguishes:

- **Desired state:** control-plane intent.
- **Observed state:** the most recent provider report.
- **Effective state:** the most recently verified result.
- **Historical state:** prior observations, relationships, and actions.

The UI and AI show timestamps and whether state is fresh, stale, offline, converging, degraded, or conflicted. An accepted command is not a successful change.

### 13.2 Durable command envelope

Every mutation carries:

- Globally unique job and operation IDs.
- Target node and resource UUIDs.
- Preconditions and intended outcome.
- Risk and approval metadata.
- Expiration time and idempotency key.
- Retry, cancellation, and verification policy.
- Structured progress and results.

Nodes persist in-progress work locally. Replayed commands do not repeat destructive operations. Expired or superseded commands are rejected after reconnecting.

### 13.3 Failure behavior

- Offline resources become stale, not deleted or empty.
- Jobs wait or expire according to declared policy.
- Partial workflows report every completed and failed step.
- Compensation is explicit; rollback is not promised where a provider cannot guarantee it.
- External changes generate drift with inspect, accept, or reconcile choices.
- Agent upgrades are staged and health-verified.
- Restored control planes reconcile before issuing mutations.

### 13.4 Structured errors

API, node, provider, plugin, automation, and AI errors share:

- Stable error code and human explanation.
- Resource, node, provider, job, and operation identifiers where applicable.
- Retryability and suggested remediation.
- Correlation ID.
- Redacted diagnostic details.

Web, mobile, CLI, automation, and AI consume the same structure. AI explanations retain original codes and evidence.

## 14. Security model

Before broad node authority is enabled, 1.0 must:

- Correct all known role/route authorization failures.
- Establish one production-enforced route and action authorization model.
- Complete the approval queue and mandatory risk behavior.
- Default-deny unknown bearer-token routes and privileged agent/plugin actions.
- Use short-lived enrollment secrets and rotatable node certificates.
- Isolate plugin execution and enforce declared capabilities.
- Redact and minimize AI, audit, log, and notification data.
- Audit human, API, automation, AI, plugin, provider, and node actions consistently.
- Preserve emergency disable and recovery paths.
- Validate prompt-injection and denylist behavior against the real action surface.

## 15. Verification and release evidence

### 15.1 Evidence levels

| Level | Required evidence |
|---|---|
| Target | Approved design or roadmap commitment |
| Source implemented | Reviewed code and unit tests |
| Contract verified | API, schema, migration, and agent compatibility tests |
| Integration verified | Real service or disposable-environment workflow |
| Failure verified | Disconnect, timeout, replay, rollback/compensation, and partial-failure tests |
| Upgrade verified | Supported prior installation upgrades successfully |
| Hardware/provider verified | Test against claimed platform or hardware |
| Release verified | Packaged artifact passes clean install, operation, backup, and recovery |

### 15.2 Required 1.0 environments and flows

- Combined single-machine deployment.
- Separate control plane with multiple Linux nodes.
- Mixed x86-64 and ARM64 nodes where claimed.
- Docker and App Vault deployment across nodes.
- Native KVM/libvirt provisioning.
- Real multi-node Proxmox and Proxmox Backup Server workflows.
- Windows and macOS endpoint enrollment.
- Disk removal, reinsertion, and cross-host correlation.
- Offline nodes, expired jobs, and reconnect reconciliation.
- Control-plane backup and replacement-host restoration.
- Automation interruption, retry, replay, cancellation, and partial failure.
- Plugin crash, timeout, upgrade, and capability denial.
- AI read, report, proposal, approval, denial, verification, and audit.
- iOS/Android authentication, pairing, notifications, deep links, offline behavior, and accessibility.
- Upgrade from the supported pre-1.0 schema and configuration.

TrueNAS, GPU passthrough, storage hardware, external AI, and other environment-specific claims require corresponding real evidence or an experimental label.

## 16. Foundation-first release train

These milestones are internal dependency gates. All are part of the 1.0 target.

### Milestone 0 — Product truth

- Freeze speculative expansion temporarily.
- Generate or verify route, role, catalog, tool, and capability counts.
- Classify claims by evidence level.
- Resolve backend, web, Tauri, and mobile version identity.
- Replace stale roadmap status with this design and an executable plan.

### Milestone 1 — Secure, durable control plane

- Authorization repair and production enforcement.
- Approval queue and unified mutation path.
- Durable jobs, events, verification, and audit.
- Numbered database migrations.
- Secrets, certificates, tokens, redaction, backup, and recovery.
- Clean install, upgrade, and golden-path CI.
- Foundation AI schemas, provenance, reports, and redaction contracts.

### Milestone 2 — CMDB and node foundation

- Canonical assets, discovery, correlation, and history.
- Secure agent protocol, enrollment, capabilities, inventory, and node jobs.
- Local and remote operations through the same contract.
- Disk identity and cross-host acceptance flow.
- Cluster inventory and node health UI/mobile/AI parity.

### Milestone 3 — Managed infrastructure cluster

- Node-aware Docker and App Vault.
- Native KVM/libvirt provisioning.
- Full Proxmox and PBS integration.
- Placement, quotas, storage, network, passthrough, backup, and recovery.
- Drift, reconciliation, maintenance, and community scripts.
- Multi-node integration and failure testing.

### Milestone 4 — Automation, plugins, and advanced AI

- Typed multi-node workflow engine and visual editor.
- Isolated plugin runtime and SDK.
- Capability-generated AI/tool integration across all completed domains.
- Odysseus and provider-independent AI orchestration.
- Complete action audit, approval, and injection-evaluation coverage.

### Milestone 5 — Household experience and clients

- Household identity, locations, devices, inventory, and selected integrations.
- Web, Tauri, iOS, Android, and tablet feature contracts.
- Mobile approvals, notifications, scanning, voice, AI, and household controls.
- Per-user experiences, privacy, and provider configuration.

### Milestone 6 — Release qualification

- Clean Docker and Linux/systemd installs and upgrades.
- Multi-node and control-plane disaster recovery.
- Real Proxmox, Docker, KVM, backup, mobile, and hardware/provider validation.
- Security, dependencies, accessibility, and compatibility review.
- Generated API documentation, support matrix, and operator runbooks.
- Signed artifacts, SBOMs, reproducible builds, and release notes.

## 17. System-level 1.0 acceptance criteria

VoidTower 1.0 is ready only when all of the following are demonstrated:

1. A new owner can install a combined instance, enroll additional heterogeneous nodes, and recover the control plane from a verified backup.
2. Authorization tests prove low-trust roles cannot reach privileged host, automation, AI, plugin, or provider actions.
3. Human, mobile, automation, AI, plugin, and API mutations all traverse the same policy, approval, job, verification, and audit path.
4. A physical disk moved between hosts remains one CMDB asset with correct history and relationships.
5. A user can create a VM through explicit or automatic placement on an eligible native or Proxmox provider and see a verified result.
6. An App Vault application can be planned, placed, deployed, observed, backed up, and recovered on a managed node.
7. A Proxmox cluster can be inventoried and managed across the provider scope claimed by the support matrix.
8. A multi-node automation can survive partial failure and expose exact progress, compensation, and audit evidence.
9. A community workflow can be pinned, verified, planned, approved, executed, checked, and reconciled without an unbounded trust path.
10. AI can view, explain, investigate, report on, and safely propose or execute applicable operations across every completed domain with evidence and redaction.
11. The branded household experience uses the same assets, identity, policy, jobs, events, and AI contracts as infrastructure management.
12. iOS and Android clients can pair securely, monitor the estate and household, receive notifications, approve actions, use AI, and handle offline/stale state safely.
13. Clean-install, upgrade, provider, failure, security, mobile, and recovery gates pass for every production-supported claim.

## 18. Documentation outcome

After this design is accepted as a consolidated document, `ROADMAP.md`, the gap analysis, codebase map, release checklist, and public README should be reconciled around it. Counts and capability lists should be generated from source where practical. External Odysseus behavior must be labelled separately from behavior verified inside this repository.

Implementation planning begins only after this design receives consolidated-document approval.
