# VoidTower HomeOS — Engineering Design Document

**Version:** 0.3 (Engineering Specification — agent-team execution model)
**Supersedes:** Vision Specification v0.1
**Status:** Draft for execution by a team of AI coding agents (Claude Code) under a single human architect/reviewer
**Repository:** `niwlekakan/voidtower` (branch `voidtower-aio`)

---

## 0. How to Read This Document

The v0.1 vision describes a product that would take a funded human team years to build. This document translates it into something shippable by a team of AI coding agents working in parallel under one human architect. Agent teams compress implementation labor dramatically — they do not compress human review bandwidth, integration testing against physical hardware, or the cost of architectural incoherence. This document is therefore written to be **agent-legible**: every boundary is a contract, every contract has a machine-checkable conformance gate, and the zones where agents must not act autonomously are named explicitly (§15.4).

Every feature in this document carries a **tier**:

| Tier | Meaning | Rule |
|------|---------|------|
| **T0** | Core. Ships in v1.0. | Fully specified in this document. |
| **T1** | Extended. Post-1.0, architecture must not preclude it. | Interfaces sketched, implementation deferred. |
| **T2** | Aspirational. Vision-level. | Named only. No code, no schema, no API surface reserved beyond extensibility points. |

If a feature is not explicitly tiered, it is **T2 by default**. The guiding principle from v0.1 ("as simple as a smartphone") is retained, but subordinated to a harder rule for this phase:

> **A feature goes in T0 only if its correctness can be verified by automated gates plus a bounded amount of human review, and its failure modes are recoverable on the real homelab.**

Implementation throughput is no longer the scarce resource. The scarce resources are: (1) the architect's review hours, (2) serial access to physical integration targets (one Proxmox cluster, one GTX 1080, one production Immich instance), and (3) architectural coherence across parallel workstreams. Tiering now exists to protect those three, not to ration coding effort.

### 0.1 Tier Assignment Summary

| Vision area | Tier | Rationale |
|---|---|---|
| Control plane, auth, web UI | T0 | Exists in embryo (VoidTower Rust/Axum). |
| Device agent (Linux) | T0 | Required for everything else. |
| Proxmox integration | T0 | Primary real-world use case (pve-svc01, pve-htpc01). |
| Docker/Podman app deployment | T0 | Compose-based App Vault already planned. |
| AI operator (Odysseus) + policy (Voidwatch) | T0 | Differentiator; permission ladder already designed. |
| Monitoring (health + basic metrics) | T0 | Minimal viable: agent-pushed samples, retention-limited. |
| Storage abstraction (read/report only) | T0 | Inventory + usage. No provisioning in v1. |
| Backups (app-level, restic-based) | **T0** (pulled forward) | Parallel-track capacity makes it affordable; it is also the recovery story the YOLO mode depends on. |
| Automation engine | T1 | First pull-forward candidate if a track finishes early. Trigger/condition/action DSL; no visual editor in v1. |
| Agent for Windows/macOS | T1 | Rust agent is portable; enrollment/UX work deferred. |
| Home Assistant integration | T1 | Adapter pattern; HA already handles IoT well — bridge, don't replace. |
| Plugin marketplace (remote) | T1 | v1 = local plugin directory only. |
| Multi-model AI routing | T1 | v1 = one cloud provider + Ollama, static routing table. |
| VoidMesh multi-node scheduling, live migration, self-healing | T2 | This is a distributed-systems research project. Explicitly out. |
| Kubernetes | T2 | Out. |
| Gaming platform (Steam/Sunshine/save sync) | T2 | Out of v1 entirely. GPU-passthrough VM lifecycle via Proxmox is T0; game-layer integration is not. |
| Voice interface, mobile apps | T2 | Web UI is responsive; that is the mobile story for now. |
| Networking management (DNS/DHCP/firewall/VPN provisioning) | T2 | Read-only network *discovery* is T1. Writing firewall rules from an AI-driven platform is a liability before the policy engine is mature. |
| Family profiles / screen time / parental controls | T2 | RBAC (T0) is the substrate; family UX is later. |
| Ceph, cluster HA | T2 | Out. |

---

## 1. Product Definition

### 1.1 One-sentence definition

VoidTower HomeOS is a self-hosted control plane that unifies the machines, VMs, containers, and services of a home network behind one API, one UI, and one policy-governed AI operator.

### 1.2 What v1.0 concretely is

A single binary (`voidtowerd`) running on a homelab host, plus a lightweight agent (`vt-agent`) on each managed Linux machine, providing:

1. **Inventory** — every device, VM, container, and deployed app visible as a uniform *resource* with health, metrics, logs, and actions.
2. **App deployment** — install curated applications (Jellyfin, Immich, Nextcloud, Vaultwarden, …) onto any Docker-capable managed host with one command, including volumes, networks, secrets, and reverse-proxy registration.
3. **Proxmox operations** — VM/LXC lifecycle, snapshots, clones, backups, cloud-init provisioning, GPU passthrough configuration — without opening the Proxmox UI for routine tasks.
4. **AI operation** — Odysseus can perform any of the above via structured tools, constrained by Voidwatch policy and the Observer → Assisted → Trusted → YOLO permission ladder, with a full audit trail and snapshot-before-apply semantics where supported.

### 1.3 Explicit non-goals for v1.0

- Replacing Home Assistant, TrueNAS, OPNsense, or Kubernetes.
- Managing devices that cannot run the agent or expose a supported API (Proxmox, Docker socket). IoT is reached *through* Home Assistant later (T1), never directly.
- High availability of the control plane itself. `voidtowerd` is a single instance; if it dies, the home keeps running (agents are autonomous for local health) and it restarts from SQLite.
- Multi-tenant / multi-home SaaS anything.

### 1.4 Personas (reduced honestly)

v0.1 named "completely non-technical family members" as users. For v1:

- **P1 — The Operator (you):** full access, uses AI + CLI + UI.
- **P2 — Household member:** read-mostly dashboard ("is Jellyfin up?", "restart it"), pre-approved actions only.

The "grandma installs a Minecraft server by voice" persona is T2 and does not drive v1 design decisions.

---

## 2. System Architecture

### 2.1 Topology

```
┌─────────────────────────────────────────────────────────────┐
│  Control Plane Host (pve-svc01 VM, Ubuntu)                  │
│                                                             │
│  ┌───────────────────────── voidtowerd ──────────────────┐  │
│  │                                                       │  │
│  │  HTTP/WS API (Axum) ── Web UI (embedded static SPA)   │  │
│  │        │                                              │  │
│  │  ┌─────┴──────┐  ┌──────────┐  ┌──────────────────┐   │  │
│  │  │ Core        │  │ Odysseus │  │ Voidwatch        │   │  │
│  │  │ (resources, │  │ (AI      │  │ (policy engine,  │   │  │
│  │  │  jobs,      │  │  operator│  │  permission      │   │  │
│  │  │  events)    │  │  runtime)│  │  ladder, audit)  │   │  │
│  │  └─────┬──────┘  └────┬─────┘  └────────┬─────────┘   │  │
│  │        │              │                 │             │  │
│  │  ┌─────┴──────────────┴─────────────────┴──────────┐  │  │
│  │  │ In-process Event Bus (tokio broadcast + WAL)    │  │  │
│  │  └─────┬───────────────────────────────────────────┘  │  │
│  │        │                                              │  │
│  │  ┌─────┴────────┐ ┌───────────┐ ┌──────────────────┐  │  │
│  │  │ Adapters:    │ │ Agent Hub │ │ Plugin Host      │  │  │
│  │  │ proxmox,     │ │ (WS mTLS) │ │ (child procs,    │  │  │
│  │  │ docker,      │ │           │ │  JSON-RPC/stdio) │  │  │
│  │  │ jellyfin, …  │ │           │ │                  │  │  │
│  │  └──────────────┘ └─────┬─────┘ └──────────────────┘  │  │
│  │                         │                             │  │
│  │  SQLite (WAL mode) ─────┘  Secrets store (age-enc.)   │  │
│  └───────────────────────────────────────────────────────┘  │
└──────────────────────────┬──────────────────────────────────┘
                           │  outbound WSS from agents (mTLS)
        ┌──────────────────┼───────────────────┐
   ┌────┴─────┐      ┌─────┴─────┐       ┌─────┴─────┐
   │ vt-agent │      │ vt-agent  │       │ vt-agent  │
   │ pve host │      │ Ubuntu VM │       │ HTPC      │
   └──────────┘      └───────────┘       └───────────┘
```

### 2.2 Architectural style: modular monolith

**Decision:** `voidtowerd` is a single Rust binary composed of internal crates in a Cargo workspace. There are exactly two deployable artifacts in v1: `voidtowerd` and `vt-agent`.

**Rejected alternative:** microservices per manager (Device Manager, Container Manager, etc. as separate services), as the v0.1 architecture diagram implies. Rejected because: the cost of microservices is paid at *runtime* by the operator (service discovery, inter-service auth, N processes to monitor on a homelab host), not at development time — so agent-team throughput does not change this calculus. One deployment target, no independent scaling needs. The monolith also gives parallel agent workstreams a single compile-time integration check (`cargo check --workspace`) instead of N deployment matrices. The v0.1 "managers" become **crates**, not services. Service extraction remains possible later because crates communicate through the event bus and trait interfaces, never by reaching into each other's state.

### 2.3 Cargo workspace layout

```
voidtower/
├── Cargo.toml                  # workspace
├── crates/
│   ├── vt-core/                # resource model, IDs, errors, config
│   ├── vt-store/               # SQLite persistence, migrations (sqlx)
│   ├── vt-events/              # event bus, event log, subscriptions
│   ├── vt-api/                 # Axum routers, DTOs, OpenAPI generation
│   ├── vt-auth/                # sessions, API keys, RBAC
│   ├── vt-agent-hub/           # agent WS server, protocol codec, enrollment
│   ├── vt-jobs/                # job queue, execution, retries, timeouts
│   ├── vt-adapters/
│   │   ├── proxmox/            # Proxmox VE API client + reconciler
│   │   ├── docker/             # Docker Engine API via agent relay
│   │   └── jellyfin/           # T1: media adapter (exists in AIO plan)
│   ├── vt-apps/                # App Vault: manifests, deploy planner
│   ├── vt-odysseus/            # AI runtime: providers, tool registry, sessions
│   ├── vt-voidwatch/           # policy engine, permission ladder, audit
│   ├── vt-plugin-host/         # plugin process supervisor, JSON-RPC
│   └── vt-secrets/             # encrypted secrets store
├── agent/
│   └── vt-agent/               # separate small dependency tree
├── ui/                         # SPA (SvelteKit or React), built → embedded
├── apps/                       # App Vault manifest repo (in-tree for v1)
├── plugins/                    # example + first-party plugins
└── docs/
```

**Rule:** `vt-agent` shares only `vt-proto` (protocol types) with the server — never `vt-core` — so the agent binary stays small (< 15 MB, musl static) and cross-compiles trivially.

### 2.4 Technology decisions (locked for v1)

| Concern | Choice | Rejected | Why |
|---|---|---|---|
| Server language | Rust (stable), Axum, Tokio | — | Already in progress; fits agent story. |
| Database | SQLite via `sqlx`, WAL mode | Postgres | Zero-ops, single-file backup, easily sufficient for home-scale (< 10⁴ resources, < 10² events/s). Schema written to be Postgres-portable if VoidMesh ever happens. |
| Metrics storage | SQLite ring tables with retention + downsampling | Prometheus/VictoriaMetrics | Don't run a second database in v1. Export endpoint (`/metrics`, Prometheus format) provided so users *can* attach Grafana. |
| Agent transport | WebSocket over TLS, mTLS, agent-initiated | gRPC | WS traverses NAT/reverse proxies trivially, one connection for commands+events+streams, no protobuf toolchain in agent build. |
| Wire format | JSON (serde), versioned envelope | protobuf | Debuggability wins at home scale; envelope allows later CBOR/proto negotiation. |
| Event bus | In-process `tokio::sync::broadcast` + persistent `events` table | NATS/Redis | No second process. NATS becomes the T2 answer if VoidMesh revives. |
| Plugins | Child processes, JSON-RPC 2.0 over stdio, declarative manifest | WASM component model | Debuggable with a shell script; language-agnostic day one; WASM is a T1/T2 hardening path, not a v1 prerequisite. |
| UI | Single-page app, static files embedded in binary (`rust-embed`) | SSR | One artifact to deploy. |
| Reverse proxy for apps | Caddy, managed via its admin API | Traefik, nginx | Automatic TLS, clean JSON admin API, single binary — deployed as a VoidTower-managed app itself. |
| Secrets at rest | `age` encryption, master key in `voidtower.key` file (0600) | Vault | One dev. Vault is a T2 integration. |

---

## 3. Service Boundaries

Each boundary below is an internal crate with a public trait interface. The rule that makes the monolith "modular" rather than a ball of mud:

> **Crates may depend on `vt-core`, `vt-events`, and `vt-store` traits. Feature crates never depend on each other. Cross-feature effects travel via events or the job queue.**

### 3.1 Boundary map

| Crate | Owns (state) | Exposes (interface) | Consumes |
|---|---|---|---|
| `vt-core` | Resource model, config | Types, `Resource` trait, error taxonomy | — |
| `vt-store` | SQLite schema, migrations | `Repo<T>` traits, transactions | vt-core |
| `vt-events` | Event log, subscriptions | `publish()`, `subscribe(filter)` | vt-store |
| `vt-auth` | Users, sessions, API keys, roles | Axum extractors (`AuthedUser`), `authorize(subject, action, resource)` | vt-store |
| `vt-agent-hub` | Agent registry, live connections | `AgentHandle::exec/stream/upload`, enrollment | vt-events, vt-secrets |
| `vt-jobs` | Job queue, run history | `enqueue(JobSpec) -> JobId`, status | vt-events, vt-store |
| `vt-adapters/*` | Adapter-specific cache | `Adapter` trait: `discover()`, `actions()`, `execute(action)` | vt-agent-hub (docker), HTTP (proxmox) |
| `vt-apps` | App manifests, deployments | `plan(install) -> Plan`, `apply(Plan)` | adapters, vt-secrets, vt-jobs |
| `vt-odysseus` | AI sessions, tool registry | `ToolRegistry::register`, chat/session API | everything, **only via public traits** |
| `vt-voidwatch` | Policies, mode ladder, audit log | `evaluate(ToolCall, Context) -> Verdict` | vt-store, vt-events |
| `vt-plugin-host` | Plugin procs, manifests | plugin lifecycle, tool/adapter registration bridge | vt-odysseus, vt-events |
| `vt-secrets` | Encrypted blobs | `get/put/rotate(scope, key)` | vt-store |

### 3.2 The critical boundary: Odysseus ↔ Voidwatch

Every tool call made by an AI model passes through Voidwatch **synchronously and unconditionally**. There is no code path from the AI runtime to an adapter that bypasses `evaluate()`. This is enforced structurally: adapters register tools into the `ToolRegistry` *wrapped* by the policy layer; Odysseus never holds a raw adapter handle.

```
model → tool_call → Voidwatch.evaluate() ──► Deny (with reason, logged)
                                        ├──► RequireApproval (job parked, UI/notification prompt)
                                        └──► Allow (+ mandatory pre-actions, e.g. snapshot) → Job queue → Adapter
```

The permission ladder (established convention, carried over):

| Mode | Behavior |
|---|---|
| **Observer** | Read-only tools. All mutating tools denied. |
| **Assisted** | Mutating tools allowed but every call → RequireApproval. |
| **Trusted** | Mutations on a per-resource/per-action allowlist auto-approved; destructive class (delete, wipe, detach-storage) still RequireApproval. Snapshot-before-apply mandatory where the target supports it (Proxmox snapshot, Btrfs snapshot, compose config backup). |
| **YOLO** | Auto-approve everything except an irreversibility denylist (host disk wipe, cert/key deletion, Voidwatch policy edits — those *always* require approval). |

Mode is set per **scope** (global default, overridable per device/app), not per conversation, and changing mode is itself an audited, approval-gated action.

---

## 4. Agent Protocol (`vt-proto` v1)

### 4.1 Design constraints

- Agent initiates and maintains a single outbound WSS connection (NAT/firewall friendly; control plane never needs to reach into device networks).
- Agent must run on a Raspberry Pi 1B-class device in principle (T1), so: no heavy runtimes, bounded memory, backpressure-aware streaming.
- All messages share one envelope; unknown message types are ignored (forward compatibility).

### 4.2 Enrollment (trust bootstrap)

1. Operator generates a **join token** in the UI/CLI: `vt agent enroll --server wss://vt.home.lan --token <one-time, 15 min TTL>`.
2. Agent generates a keypair, sends CSR + token over TLS (server cert verified against pinned fingerprint printed at token creation, or system CAs).
3. Server validates token, signs client cert with the internal VoidTower CA (created at first boot, stored in vt-secrets), returns cert + CA bundle.
4. All subsequent connections are mTLS. Token is burned. Device resource is created in state `pending_approval` → operator approves → `managed`.

Certificate rotation: agent renews at 2/3 lifetime via `cert.renew` RPC; certs live 90 days.

### 4.3 Envelope

```json
{
  "v": 1,
  "id": "01J8ZK3W9A4Q2R8T5Y6U7I9O0P",   // ULID, unique per message
  "re": "01J8ZK3V...",                   // optional: correlates response/stream to request id
  "ts": "2026-07-08T12:34:56.789Z",
  "kind": "req | res | event | stream | err",
  "type": "exec.run",                    // namespaced message type
  "body": { }
}
```

### 4.4 Message types (v1 surface, complete)

**Server → Agent (requests):**

| Type | Body | Semantics |
|---|---|---|
| `exec.run` | `{cmd, args[], env{}, timeout_ms, stream: bool}` | Run command. If `stream`, stdout/stderr arrive as `stream` frames (`{fd, seq, data_b64}`) then final `res` with exit code. |
| `svc.action` | `{unit, action: start\|stop\|restart\|status}` | systemd unit management. |
| `docker.proxy` | `{method, path, body_b64}` | Relay one Docker Engine API call over the agent's local socket. The docker adapter on the server is built entirely on this. |
| `file.read` / `file.write` | `{path, offset, len}` / `{path, mode, data_b64, append}` | Chunked (≤ 256 KiB/frame). Writes go to temp + atomic rename. |
| `sys.inventory` | `{}` | Full hardware/OS/software inventory document. |
| `pkg.list` | `{manager?}` | Installed packages (apt/pacman/dnf detection). |
| `power.action` | `{action: shutdown\|reboot}` | Requires Voidwatch destructive-class approval upstream. |
| `cert.renew` | `{csr_pem}` | Rotation. |
| `ping` | `{}` | Liveness; agent must reply within 10 s. |

**Agent → Server (events, unsolicited):**

| Type | Cadence | Body |
|---|---|---|
| `agent.hello` | On connect | `{agent_version, proto_v, hostname, machine_id, capabilities[]}` |
| `metrics.sample` | Every 15 s (configurable) | `{cpu_pct, mem{}, disks[], net[], temps[], gpu?}` — one compact frame, server downsamples. |
| `inventory.delta` | On change (debounced 60 s) | Changed inventory sections only. |
| `docker.event` | Real-time | Relayed Docker events stream (container start/die/oom). |
| `journal.alert` | Real-time, filtered | systemd units entering failed state, OOM-kills. |
| `agent.goodbye` | On clean shutdown | `{reason}` |

### 4.5 Reliability semantics

- **Commands are at-most-once.** The server never blindly retries `exec.run`; retry decisions belong to the job layer, which first queries idempotency (e.g., "is the container already running?") via read operations.
- Agent buffers up to 5 MB of outbound events during disconnects (ring buffer, oldest-dropped, `metrics.sample` dropped first, `journal.alert` last).
- Server marks a device `unreachable` after 3 missed pings (~45 s) and emits `device.state_changed`.
- Capability negotiation: server only sends message types the agent listed in `capabilities[]`, so old agents keep working against new servers.

### 4.6 Agent hard limits

- Static musl binary, target < 15 MB, RSS target < 30 MB steady-state.
- No plugin system in the agent (v1). All extensibility is server-side; the agent is a dumb, audited pipe with a small fixed verb set. This is a security decision as much as a scope one.

---

## 5. API Contracts

### 5.1 Conventions

- Base path `/api/v1`. JSON only. OpenAPI 3.1 spec generated from code (`utoipa`) and served at `/api/v1/openapi.json`.
- IDs are ULIDs, prefixed by type for debuggability: `dev_01J8…`, `app_01J8…`, `job_01J8…`, `vm_…`, `ct_…`, `evt_…`, `usr_…`.
- Errors (RFC 7807 problem+json):

```json
{
  "type": "https://voidtower.dev/errors/policy-denied",
  "title": "Denied by Voidwatch policy",
  "status": 403,
  "detail": "power.action(shutdown) on dev_01J8… requires approval in Assisted mode",
  "trace_id": "…",
  "denial": { "policy": "mode:assisted", "approval_id": "apr_01J8…" }
}
```

- Pagination: cursor-based, `?limit=50&cursor=…`, response carries `next_cursor`.
- Every mutating endpoint accepts `Idempotency-Key` header; keys stored 24 h.
- Long-running work returns `202 Accepted` + `{job_id}`; poll `/jobs/{id}` or subscribe via WS.

### 5.2 Resource surface (T0, complete)

```
Auth
  POST   /auth/login                     → session cookie
  POST   /auth/logout
  GET    /auth/me
  POST   /auth/api-keys                  → {token} (shown once)
  DELETE /auth/api-keys/{id}

Resources (uniform layer — works for every resource type)
  GET    /resources?type=&tag=&health=&q=
  GET    /resources/{id}                 → full resource doc incl. available actions
  GET    /resources/{id}/events?since=
  GET    /resources/{id}/metrics?series=cpu&from=&to=&step=
  POST   /resources/{id}/actions/{verb}  → 202 {job_id}   # generic action dispatch
  PATCH  /resources/{id}                 → name, description, tags, owner

Devices
  GET    /devices
  POST   /devices/enroll-tokens          → {token, expires_at, ca_fingerprint}
  POST   /devices/{id}/approve
  DELETE /devices/{id}                   # decommission (destructive class)
  GET    /devices/{id}/terminal          # WS upgrade → interactive PTY via agent

Proxmox
  GET    /proxmox/nodes
  GET    /proxmox/vms  /proxmox/cts
  POST   /proxmox/vms                    → create from template/cloud-init spec
  POST   /proxmox/vms/{id}/actions/{start|stop|shutdown|reboot|suspend}
  POST   /proxmox/vms/{id}/snapshots     / DELETE …/snapshots/{name} / POST …/rollback
  POST   /proxmox/vms/{id}/clone
  POST   /proxmox/vms/{id}/migrate       # T1 (needs cluster)
  GET    /proxmox/storage

Apps (App Vault)
  GET    /apps/catalog                   → available manifests + versions
  GET    /apps/catalog/{slug}
  POST   /apps/installs                  → {slug, target_device, config{}} → plan preview
  POST   /apps/installs/{id}/apply       → 202 {job_id}
  GET    /apps                           → deployed apps
  POST   /apps/{id}/actions/{start|stop|restart|update|backup}
  DELETE /apps/{id}?keep_data=true

Jobs
  GET    /jobs?state=&resource=
  GET    /jobs/{id}                      → spec, state, steps, logs
  POST   /jobs/{id}/cancel

Approvals (Voidwatch)
  GET    /approvals?state=pending
  POST   /approvals/{id}/decision        → {approve: bool, comment}

AI (Odysseus)
  POST   /ai/sessions                    → {session_id}
  POST   /ai/sessions/{id}/messages      → SSE stream of deltas + tool events
  GET    /ai/sessions/{id}
  GET    /ai/tools                       → registered tool schemas + current policy verdict preview
  GET/PUT /ai/config                     → providers, routing table, default mode

Policy & audit
  GET/PUT /voidwatch/modes               → {global, overrides: {resource_id: mode}}
  GET    /voidwatch/policies  (T1: PUT custom rules)
  GET    /audit?actor=&resource=&from=   → append-only audit entries

Events & realtime
  GET    /events?since=&type=            # history (from event log)
  WS     /ws                             # subscribe: {"sub": ["events", "jobs:job_…", "metrics:dev_…"]}

System
  GET    /system/health                  → component checks (db, agents, adapters, ai providers)
  GET    /metrics                        → Prometheus exposition (server + latest agent samples)
```

### 5.3 WebSocket event frames (server → client)

```json
{ "channel": "events", "event": {
    "id": "evt_01J8…", "ts": "…",
    "type": "app.deployed",
    "resource_id": "app_01J8…",
    "actor": { "kind": "ai", "session": "ais_01J8…", "approved_by": "usr_01J8…" },
    "data": { "slug": "jellyfin", "version": "10.10.3", "device": "dev_01J8…" }
} }
```

`actor.kind ∈ {user, ai, system, plugin, schedule}` on **every** event and audit row — provenance is non-optional.

### 5.4 Versioning & stability policy

- `/api/v1` is stable after 1.0: additive changes only (new fields, new endpoints). Breaking → `/api/v2` with a one-minor-release overlap.
- The agent protocol versions independently (`proto_v` in hello); server supports current + previous.
- Plugin SDK versions independently (manifest `sdk: 1`).

---

## 6. Data Model & Database Schema

### 6.1 Modeling decision

The v0.1 "everything inherits from Resource" idea survives, implemented as **one narrow `resources` table + type-specific detail tables** (not a single JSON blob table, not table-per-type duplication of common fields). Uniform queries ("all unhealthy things") hit `resources`; typed operations join their detail table.

### 6.2 Schema (SQLite, sqlx migrations; Postgres-compatible types)

```sql
-- 0001_core.sql
CREATE TABLE resources (
  id          TEXT PRIMARY KEY,             -- typed ULID: dev_…, app_…
  kind        TEXT NOT NULL,                -- device|vm|ct|container|app|volume|automation|user|…
  name        TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  owner_id    TEXT REFERENCES resources(id),
  parent_id   TEXT REFERENCES resources(id),-- container→device, vm→pve node
  health      TEXT NOT NULL DEFAULT 'unknown', -- ok|degraded|failed|unknown
  status      TEXT NOT NULL DEFAULT 'unknown', -- kind-specific lifecycle string
  tags        TEXT NOT NULL DEFAULT '[]',   -- JSON array
  attrs       TEXT NOT NULL DEFAULT '{}',   -- JSON: non-indexed, kind-specific extras
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  deleted_at  TEXT                          -- soft delete; hard-purge job after 30 d
);
CREATE INDEX idx_resources_kind_health ON resources(kind, health) WHERE deleted_at IS NULL;

CREATE TABLE relationships (                -- typed graph edges (Cognitio heritage)
  from_id TEXT NOT NULL REFERENCES resources(id),
  to_id   TEXT NOT NULL REFERENCES resources(id),
  rel     TEXT NOT NULL,                    -- runs_on|stores_on|proxied_by|depends_on|backs_up_to
  PRIMARY KEY (from_id, rel, to_id)
);

CREATE TABLE devices (
  resource_id     TEXT PRIMARY KEY REFERENCES resources(id),
  machine_id      TEXT UNIQUE NOT NULL,
  hostname        TEXT NOT NULL,
  os              TEXT, arch TEXT, kernel TEXT,
  agent_version   TEXT,
  cert_serial     TEXT, cert_expires_at TEXT,
  capabilities    TEXT NOT NULL DEFAULT '[]',
  last_seen_at    TEXT,
  enroll_state    TEXT NOT NULL DEFAULT 'pending_approval',
  inventory       TEXT NOT NULL DEFAULT '{}'   -- latest full inventory JSON
);

CREATE TABLE apps (
  resource_id   TEXT PRIMARY KEY REFERENCES resources(id),
  slug          TEXT NOT NULL,
  version       TEXT NOT NULL,
  device_id     TEXT NOT NULL REFERENCES resources(id),
  config        TEXT NOT NULL DEFAULT '{}',    -- user-supplied answers (non-secret)
  compose_hash  TEXT NOT NULL,                 -- rendered artifact fingerprint
  url           TEXT,                          -- as registered with reverse proxy
  data_paths    TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE pve_endpoints (
  resource_id TEXT PRIMARY KEY REFERENCES resources(id),
  base_url    TEXT NOT NULL,
  token_ref   TEXT NOT NULL,                  -- key into secrets store
  fingerprint TEXT,
  last_sync   TEXT
);
-- pve VMs/CTs live in `resources` (kind='vm'|'ct', parent = node device) +
CREATE TABLE pve_guests (
  resource_id TEXT PRIMARY KEY REFERENCES resources(id),
  vmid        INTEGER NOT NULL,
  node        TEXT NOT NULL,
  guest_type  TEXT NOT NULL,                  -- qemu|lxc
  spec_cache  TEXT NOT NULL DEFAULT '{}',
  UNIQUE (node, vmid)
);
```

```sql
-- 0002_events_jobs.sql
CREATE TABLE events (                          -- append-only, also the bus WAL
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  id          TEXT UNIQUE NOT NULL,
  ts          TEXT NOT NULL,
  type        TEXT NOT NULL,
  resource_id TEXT,
  actor       TEXT NOT NULL,                   -- JSON {kind, id, session?, approved_by?}
  data        TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_events_resource ON events(resource_id, seq);
CREATE INDEX idx_events_type     ON events(type, seq);

CREATE TABLE jobs (
  id          TEXT PRIMARY KEY,
  kind        TEXT NOT NULL,                   -- app.install|vm.create|exec|…
  resource_id TEXT,
  spec        TEXT NOT NULL,
  state       TEXT NOT NULL DEFAULT 'queued',  -- queued|running|waiting_approval|succeeded|failed|cancelled
  steps       TEXT NOT NULL DEFAULT '[]',      -- [{name, state, started_at, log_ref}]
  error       TEXT,
  actor       TEXT NOT NULL,
  idem_key    TEXT UNIQUE,
  created_at  TEXT NOT NULL, started_at TEXT, finished_at TEXT,
  attempts    INTEGER NOT NULL DEFAULT 0, max_attempts INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE approvals (
  id          TEXT PRIMARY KEY,
  job_id      TEXT NOT NULL REFERENCES jobs(id),
  tool_call   TEXT NOT NULL,                   -- JSON: name, args, policy verdict detail
  risk_class  TEXT NOT NULL,                   -- read|mutate|destructive|irreversible
  state       TEXT NOT NULL DEFAULT 'pending', -- pending|approved|denied|expired
  decided_by  TEXT, decided_at TEXT, comment TEXT,
  expires_at  TEXT NOT NULL                    -- default 24 h → expired = denied
);
```

```sql
-- 0003_auth_secrets_audit.sql
CREATE TABLE users (
  resource_id   TEXT PRIMARY KEY REFERENCES resources(id),
  username      TEXT UNIQUE NOT NULL,
  password_hash TEXT NOT NULL,                 -- argon2id
  role          TEXT NOT NULL DEFAULT 'member',-- operator|member|viewer
  totp_secret_ref TEXT,
  disabled      INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE sessions (
  id TEXT PRIMARY KEY, user_id TEXT NOT NULL, created_at TEXT NOT NULL,
  expires_at TEXT NOT NULL, ip TEXT, ua TEXT
);
CREATE TABLE api_keys (
  id TEXT PRIMARY KEY, user_id TEXT NOT NULL, name TEXT NOT NULL,
  token_hash TEXT UNIQUE NOT NULL, scopes TEXT NOT NULL DEFAULT '[]',
  last_used_at TEXT, expires_at TEXT
);
CREATE TABLE secrets (
  scope TEXT NOT NULL, key TEXT NOT NULL,      -- scope: global | app:app_… | device:dev_…
  ciphertext BLOB NOT NULL,                    -- age-encrypted
  created_at TEXT NOT NULL, rotated_at TEXT,
  PRIMARY KEY (scope, key)
);
CREATE TABLE audit_log (                       -- distinct from events: security-relevant, never pruned
  seq INTEGER PRIMARY KEY AUTOINCREMENT,
  ts TEXT NOT NULL, actor TEXT NOT NULL, action TEXT NOT NULL,
  resource_id TEXT, verdict TEXT,              -- allow|deny|approved|…
  detail TEXT NOT NULL DEFAULT '{}',
  prev_hash TEXT NOT NULL, hash TEXT NOT NULL  -- hash chain: tamper-evident
);
```

```sql
-- 0004_metrics.sql  (ring storage, aggressively bounded)
CREATE TABLE metrics_raw (                     -- 15 s resolution, kept 24 h
  device_id TEXT NOT NULL, ts INTEGER NOT NULL, series TEXT NOT NULL, value REAL NOT NULL,
  PRIMARY KEY (device_id, series, ts)
) WITHOUT ROWID;
CREATE TABLE metrics_5m (                      -- 5 min avg/min/max, kept 30 d
  device_id TEXT NOT NULL, ts INTEGER NOT NULL, series TEXT NOT NULL,
  avg REAL, min REAL, max REAL,
  PRIMARY KEY (device_id, series, ts)
) WITHOUT ROWID;
-- downsample + prune run as system jobs hourly
```

### 6.3 Data policies

- Events pruned at 90 days / 1 M rows (whichever first); audit_log never pruned.
- Nightly `VACUUM INTO` snapshot of the DB to the backup path + optional restic push (T1); DB file lives on the Btrfs subvolume so snapshot-before-upgrade is free.
- All timestamps UTC RFC 3339; SQLite `TEXT` for portability.

---

## 7. Event Bus Design

### 7.1 Requirements

1. Fan-out server-internal domain events to: WS clients, Odysseus (context awareness), automation engine (T1), plugin host.
2. Durable history for `GET /events` and for replay after restart.
3. Zero additional infrastructure.

### 7.2 Design

- **Hot path:** `tokio::sync::broadcast::Sender<Arc<Event>>` (capacity 4096). Publishing is: (a) synchronous insert into `events` table (same transaction as the state change where applicable — the event is the commit record), then (b) broadcast.
- **Slow consumers** (a lagging WS client) get `Lagged(n)` from broadcast and resync from the `events` table using their last `seq`. The table *is* the source of truth; the channel is just a doorbell.
- **Filters:** subscription = `EventFilter { types: glob list, resource_ids, actor_kinds }`, evaluated consumer-side in `vt-events` to keep the publisher O(1).
- **Naming:** `domain.entity_verb` past tense: `device.enrolled`, `device.state_changed`, `vm.snapshot_created`, `app.deploy_failed`, `ai.tool_called`, `voidwatch.approval_requested`, `job.state_changed`. Registry of all event types lives in `vt-events::catalog` — a compile-time enum, not stringly-typed at publish sites.
- **Delivery guarantee:** at-least-once for pull consumers (via seq cursor), best-effort for push. Consumers must be idempotent on event id.

### 7.3 Deliberate non-features

No cross-host bus, no partitioning, no consumer groups, no exactly-once. If VoidMesh (T2) revives, the `vt-events` trait boundary is where NATS JetStream slots in.

---

## 8. Plugin SDK

### 8.1 Model

A plugin is a **directory** containing a manifest and an executable. The plugin host launches it as a child process and speaks **JSON-RPC 2.0 over stdio** (LSP-style, `Content-Length` framed). Any language; a plugin can be a 40-line Python script.

```
plugins/hello-weather/
├── plugin.toml
└── run            (executable)
```

```toml
# plugin.toml
sdk = 1
id = "hello-weather"
name = "Weather Tools"
version = "0.1.0"
entrypoint = "./run"

[capabilities]           # everything is opt-in, enforced by host
tools = true             # may register AI tools
events = ["app.*"]       # event types it may subscribe to
http_ui = false          # T1: serve a UI panel
adapter = false          # T1: full resource adapter

[permissions]            # operator approves this set at install time
network = ["api.open-meteo.com:443"]   # host-enforced later (T1: netns); v1 = declared + audited
secrets = ["plugin:hello-weather/*"]
resources = { read = ["device"], actions = [] }
```

### 8.2 RPC surface (v1)

**Host → plugin:** `initialize{host_version, granted_permissions}`, `shutdown`, `tool.invoke{name, args, context{actor, mode, resource?}}`, `event.deliver{event}`, `health.check`.

**Plugin → host:** `tool.register{name, description, json_schema, risk_class}`, `resource.get/list{...}` (scoped by permissions), `secret.get{key}`, `event.publish{type: "plugin.hello-weather.*", data}` (namespaced — plugins cannot forge core events), `log{level, msg}`.

### 8.3 Host-side rules

- Plugin-registered AI tools pass through **Voidwatch like every other tool** — a plugin cannot escalate past the current mode, and its declared `risk_class` is a floor the host may raise, never lower.
- Supervision: crash → restart with exponential backoff, 5 crashes/10 min → disabled + `plugin.failed` event.
- Resource ceilings via cgroups where available (memory 256 MB default).
- v1 isolation is honest: **process + permission-checked RPC + audit**, not a sandbox. The manifest permission model is designed so the WASM/sandbox upgrade (T1/T2) tightens enforcement without changing plugin-facing APIs.
- First-party plugins shipped in-tree double as SDK examples and conformance tests.

### 8.4 App Vault manifests (related but distinct)

Apps are *data*, not code — no process, no RPC:

```toml
# apps/jellyfin/app.toml
slug = "jellyfin"; name = "Jellyfin"; category = "media"
versions = ["10.10.3"]
compose = "compose.yaml.tera"        # Tera template
[questions.media_path]
prompt = "Where is your media library?"
type = "host_path"; default = "/tank/media"
[proxy]
subdomain_default = "jellyfin"; port = 8096
[health]
http = "/health"
[backup]
paths = ["{data}/config"]           # consumed by T1 restic integration
```

Deploy pipeline: render template → plan (show diff: images, volumes, ports, proxy route, generated secrets) → approval (per mode) → agent `docker.proxy` apply → health gate → `app.deployed`.


---

## 9. AI Orchestration (Odysseus)

### 9.1 v1 scope — deliberately narrow

The v0.1 "route coding→Claude, research→GPT, creative→Gemini" engine is T1. v1 ships:

- **Two providers:** one cloud provider (Anthropic API) + Ollama (local, already running qwen2.5-coder on pve-htpc01).
- **Static routing table** in config, keyed by task class, editable via `/ai/config`:

```toml
[ai.routing]
default        = "anthropic/claude-sonnet"
local_only     = "ollama/qwen2.5-coder:14b-instruct"   # used when session is marked private
summarize      = "ollama/qwen2.5-coder:7b"             # cheap background tasks (event digests)
```

- Provider abstraction is one trait (`ChatProvider: complete(messages, tools) -> Stream<Delta>`), so adding OpenAI/OpenRouter later is a crate module, not an architecture change.

### 9.2 Tool registry

- Adapters and plugins register tools with: name, description, JSON Schema args, `risk_class`, and a `preview(args) -> HumanSummary` function used in approval prompts ("This will **stop VM 104 (cachyos-gaming)** on sessrumnir").
- Context injection per session: current resource inventory summary (bounded), recent relevant events, active mode. Token budget enforced (inventory summarizer runs as a background job using the local model).

### 9.3 Session semantics

- Sessions are persistent resources (`ais_…`) with full message + tool-call history (feeds audit).
- Every tool call emits `ai.tool_called` / `ai.tool_result` events → live-visible in the UI ("Odysseus is checking disk usage on pve-svc01…").
- Hard stops: max 25 tool calls per user turn; any `destructive` tool call in a chain forces re-approval even in Trusted mode; cancellation kills in-flight jobs it spawned (where the job kind is cancellable).

### 9.4 Prompt-injection posture

Agent output, container logs, and app metadata are **untrusted input** to the model. Mitigations (v1): tool results are wrapped in delimiters with an instruction-hierarchy system prompt; Voidwatch evaluates tool *calls* regardless of why the model made them (policy is the backstop, not the model's judgment); `journal.alert`/log-derived context is truncated and never allowed to trigger auto-approved destructive actions (log-triggered chains cap at `mutate` class).

---

## 10. Security Model

### 10.1 Threat model (v1, explicit)

**In scope:** compromised/malicious LAN device; prompt injection steering the AI; a compromised agent host attempting lateral movement through VoidTower; stolen session cookie; malicious third-party plugin (partially — see 8.3); accidental destructive operations (the most likely incident in practice).

**Out of scope (v1):** nation-state, malicious operator, physical access to the control-plane host, side channels.

### 10.2 Controls

| Layer | Control |
|---|---|
| Transport | TLS everywhere. Agent links mTLS with internal CA. UI via Caddy-managed cert or internal CA. |
| AuthN | Argon2id passwords, TOTP MFA (operator role mandatory at 1.0), API keys hashed (SHA-256) with scopes. Sessions: HttpOnly, SameSite=Strict, 7-day sliding. |
| AuthZ | RBAC: `operator` (all), `member` (read + allowlisted actions), `viewer` (read). Enforced in `vt-auth` extractors *and* rechecked in Voidwatch for AI-initiated calls. |
| Blast radius | Agents have no inbound ports; agent verb set is small and fixed; per-device Voidwatch mode overrides let you keep e.g. the Proxmox *host* agent in Assisted even when global mode is Trusted. |
| Secrets | age-encrypted at rest; master key file 0600, outside the DB, excluded from DB backups (documented recovery procedure); secrets never appear in events, job logs, or AI context (redaction middleware on the tool-result path, tested). |
| Audit | Hash-chained `audit_log` for every auth event, mode change, approval decision, destructive action, secret access. `vt audit verify` CLI validates the chain. |
| Supply chain | `cargo-deny` (licenses, advisories) + `cargo-audit` in CI; lockfiles committed; release binaries built in CI only, checksummed, signed with minisign. |
| Updates | `voidtowerd` self-update is manual in v1 (systemd + documented procedure); agent update pushed from server (verify minisign signature → swap → restart), staged one device at a time with health gate. |

### 10.3 The irreversibility denylist (Voidwatch, hardcoded)

Regardless of mode — including YOLO — these always require human approval: host power actions on the control-plane's own host; deletion of the last snapshot/backup of a resource; `keep_data=false` app removal; device decommission; Voidwatch policy/mode changes; secrets master-key operations; agent update rollouts.

---

## 11. Monitoring & Observability (of and by the platform)

- **Platform self-observability:** `tracing` with JSON output, trace IDs propagated API→job→agent command; `/system/health` component checks; `/metrics` Prometheus exposition.
- **Home observability (the product feature):** health rollup on the dashboard is computed, not raw — per-resource health from typed checks (agent liveness, container state + health endpoint, PVE status, disk fill projection ["/tank full in ~9 days at current rate"], SMART attributes via agent, cert expiry, backup staleness). Raw graphs exist but are one click down, per the v0.1 "insights over graphs" principle.
- Alert routing v1: in-UI + ntfy/webhook push. Email/Signal/etc. are plugins.

---

## 12. CI/CD Strategy

GitHub Actions, three workflows:

**`ci.yml`** — every push/PR: `cargo fmt --check` → `cargo clippy --workspace --all-targets -- -D warnings` → `cargo test --workspace` → `cargo deny check` → UI `lint + typecheck + vitest` → build server (x86_64-gnu) + agent matrix (x86_64-musl, aarch64-musl, armv7-musl) → **protocol conformance suite** (spins server + real agent binary in the runner, exercises every `vt-proto` message type) → integration tests (testcontainers: real Docker deploy of a sample app manifest end-to-end).

**`release.yml`** — on tag `v*`: reproducible builds, SBOM (cargo-sbom), minisign signatures, GitHub Release with checksums, container image for `voidtowerd` (ghcr), `install.sh` + one-command self-host script.

**`nightly.yml`** — `cargo audit`, dependency-update PRs (Renovate), long-running soak test: 3 simulated agents, 24 h synthetic event load, assert memory ceiling + zero event-log gaps.

Branch policy: trunk-based, `main` always releasable, every agent workstream merges via PR with CI required. CI is the first reviewer for all code; the human is the second reviewer at the tier defined in §15.5 — the conformance suites exist precisely so that most agent-written code can be trusted through gates rather than line-by-line reading. Versioning: SemVer; pre-1.0 minor = breaking.

**Definition of Done (every PR):** code + tests + docs updated + OpenAPI regenerated & committed + changelog entry + no new clippy allows without a comment justifying it.

---

## 13. Testing Requirements

| Level | Scope | Tooling | Gate |
|---|---|---|---|
| Unit | Policy engine (highest bar: **every mode × risk_class × denylist combination table-tested**), planners, template rendering, protocol codec | `cargo test`, `insta` snapshots for plans/prompts | 90% line coverage on `vt-voidwatch`; no global target elsewhere — coverage where it pays |
| Property | Protocol envelope roundtrip, event filter matching, ULID ordering | `proptest` | CI |
| Contract | Agent protocol: golden transcripts per message type, replayed against both real server and real agent | conformance suite (own harness) | CI, blocks merge |
| Integration | App deploy end-to-end against real Docker; Proxmox adapter against **recorded fixtures** (wiremock) in CI + a `just test-pve-live` target run manually against the real homelab before release | testcontainers, wiremock | CI (mocked), manual (live) |
| AI evals | Scenario suite: N tasks ("install jellyfin on X", "why is immich down?") run against the tool registry with a mock model asserting *tool-call sequences and policy verdicts*, not prose | own harness | CI |
| Security | Redaction tests (secrets never in logs/events/AI context), authz matrix per endpoint, audit-chain verification | `cargo test` | CI, blocks merge |
| Upgrade | Migrate a seeded previous-version DB → assert invariants | CI on release branches | Release gate |
| Manual release checklist | Fresh install on clean VM; enroll agent; deploy 2 apps; Odysseus session in each mode; restore DB from backup | documented runbook | Release gate |

---

## 14. Coding Standards

- **Toolchain:** pinned stable via `rust-toolchain.toml`; `rustfmt` defaults; clippy pedantic-leaning set, `-D warnings` in CI.
- **Errors:** each crate defines errors with `thiserror`; `anyhow` only in binaries/tests. Every error crossing the API boundary maps to a typed problem+json `type`. No `unwrap`/`expect` outside tests and provably-infallible cases (comment required).
- **Async:** Tokio; no blocking calls on the runtime (`spawn_blocking` for SQLite-heavy paths is handled inside `vt-store` — callers never think about it). All external I/O has explicit timeouts. Cancellation-safety documented on every public async fn that isn't.
- **Boundaries:** the §3 dependency rule is enforced with `cargo-machete` + a CI check on the workspace dependency graph.
- **API DTOs** live in `vt-api`, separate from domain types — no `sqlx::FromRow` structs serialized to clients.
- **Unsafe:** forbidden (`#![forbid(unsafe_code)]`) except `vt-agent` platform shims, each with a `// SAFETY:` justification.
- **Commits:** Conventional Commits (`feat:`, `fix:`, `refactor:`…) — changelog is generated.
- **Docs:** every crate has a `//!` overview; every public trait documents its contract (idempotency, cancellation, error semantics). ADRs (Architecture Decision Records) in `docs/adr/` — the decisions in this document become ADR-001…-012 at kickoff.
- **UI:** TypeScript strict; API client generated from OpenAPI (no hand-written fetch paths); component states (loading/error/empty) mandatory.

---

## 15. Development Model — AI Agent Team

This project is implemented by a team of Claude Code agents working in parallel, under a single human architect. This section is the operating contract for that team. It is written on one premise: **agent throughput is effectively unlimited; verification is not.** Everything below exists to convert cheap implementation labor into trustworthy software without saturating the one human in the loop.

### 15.1 Roles

| Role | Held by | Authority |
|---|---|---|
| **Architect** (Ewwi) | Human | Owns this EDD and all ADRs. Sole merge authority to `main`. Sole access to production homelab and its credentials. Runs all G4 gates (§15.3). Decides tier changes and pull-forwards. |
| **Orchestrator** | 1 agent session | Decomposes milestones into task specs (§15.2), tracks the dependency graph, assigns work, files integration issues. Writes no product code. |
| **Workers** | N agent sessions | One task at a time, one crate at a time, fresh session per task. Implement to spec, produce PR with tests + docs. |
| **Adversarial reviewer** | 1 agent session per PR | Reviews with **no shared context with the author session**, briefed only with the EDD, the task spec, and the diff. Mandate: find spec deviations, missing failure paths, tests that don't test. |
| **Integrator** | 1 agent session | Rebases green PRs, resolves mechanical conflicts, runs the full workspace suite, queues the human merge list. |

### 15.2 Task specs

No agent starts work from a vibe. Every task is a written spec containing: the crate and module touched; the contract (trait signatures, message types, or endpoints — copied from this EDD, not paraphrased); acceptance tests **named in advance**; explicit forbidden zones for this task; and an estimated review tier (§15.5). If an agent discovers mid-task that the spec is wrong, it stops and files a spec issue — it does not improvise architecture.

### 15.3 Verification gates

"Confirmed working" is defined mechanically. A change is *done* when it has passed every gate applicable to it:

| Gate | What | Run by | Blocking |
|---|---|---|---|
| **G0** | `fmt`, `clippy -D warnings`, `cargo deny`, workspace dep-graph rule (§3) | CI | merge |
| **G1** | Unit + property tests; **mutation testing** (`cargo-mutants`) on `vt-voidwatch`, `vt-proto`, `vt-auth` — kills the "tests that assert nothing" failure mode | CI | merge |
| **G2** | Conformance: protocol golden transcripts, policy mode×risk matrix, OpenAPI diff (breaking change → auto-fail), redaction suite | CI | merge |
| **G3** | Integration in the forge (§15.6): real Docker deploys, nested-PVE adapter runs, soak jobs | CI (forge runner) | merge |
| **G4** | Human: security-tier line review (§15.5), live-hardware checklist against the real homelab, release sign-off | Architect | merge to `main` / release |

**Agent review certifies consistency; gates G1–G3 certify behavior in controlled environments; only G4 certifies reality.** A thousand green agent-hours do not substitute for G4 — they exist to make G4 short.

### 15.4 Forbidden zones

Agents may not modify the following without a human-approved ADR, enforced by CODEOWNERS + CI path protection:

1. Database migrations and the schema in §6
2. `vt-proto` message types or envelope semantics
3. Voidwatch semantics: the mode ladder, risk classes, and above all the irreversibility denylist (§10.3)
4. Cryptography and credential handling in `vt-auth` / `vt-secrets` / agent enrollment
5. **CI workflow and gate definitions** — an agent blocked by a gate amends its code or files a spec issue; it never amends the gate
6. This EDD and the ADRs

Item 5 is the load-bearing one for unattended operation: the most common failure of long-horizon autonomous runs is an agent "unblocking" itself by weakening its own verification.

### 15.5 Review tiering (protecting the human bottleneck)

| Tier | Code | Human involvement |
|---|---|---|
| **Full line review** | `vt-voidwatch`, `vt-auth`, `vt-secrets`, `vt-agent`, migrations, CI | Every line, every PR |
| **Boundary review** | Adapters, `vt-api`, `vt-agent-hub`, `vt-jobs`, `vt-apps` | Human reads public interfaces, error paths, and the adversarial reviewer's report; body trusted to gates |
| **Gate-trusted** | UI components, docs, first-party plugins, test scaffolding | Sampled; gates + adversarial review carry it |

### 15.6 Operating rhythm and the forge sandbox

**Rhythm — bursts, not perpetual motion.** Agents run in high-autonomy batches (including overnight) *only* on tasks with complete specs and applicable G0–G3 gates. Every 24 h has a hard human checkpoint: triage the merge queue, run any pending G4 items, amend specs, requeue failures. Continuous multi-week unattended operation is explicitly rejected: context degrades, conventions fork, and failure compounds silently. Fresh session per task; per-crate `CLAUDE.md` files carry conventions so no session depends on another session's memory. Token/subscription spend is tracked per milestone like any other resource.

**The forge.** All agent execution happens inside **`vt-forge`**, a dedicated VM on pve-svc01:

- VM (not LXC — nested virtualization and clean Docker required), 8–16 GB RAM, dedicated Btrfs/PVE **snapshot before every unattended batch**; a bad run is a rollback + amended spec, never a cleanup archaeology session.
- Contains: repo checkout, Rust/Node toolchains, Docker, Claude Code running in permissive mode — which is acceptable *only because of* this isolation (the same blast-radius reasoning as the Voidwatch ladder itself).
- **Holds zero production credentials.** No real PVE tokens, no SSH keys to sessrumnir or the services VM, no access to Immich/Nextcloud data. The Proxmox adapter integrates against a **nested PVE instance inside/beside the forge**; app-deploy tests run against the forge's own Docker daemon.
- Network egress allowlisted: crates.io, npm registry, GitHub, api.anthropic.com, container registries. The forge cannot reach the production LAN segment.
- **Git is the only artifact channel.** Agents push review branches; merges to `main` and all production deployments happen outside the forge, human-initiated, from tagged releases only.

---

## 16. Phased Roadmap (parallel tracks)

Calendar time is now bounded by G4 review hours and serial access to real hardware, not implementation. Ranges assume roughly 5–10 architect hours/week on gates and specs. **Every phase still ends with a tagged release running in production on pve-svc01 — dogfooding remains the only real QA.**

### Phase 0 — Spine (serial, human-heavy: 2–3 weeks)
Cannot parallelize: everything depends on these contracts. Workspace per §2.3; `vt-core` resource model; store + migrations; event bus; auth + RBAC; `/resources`, `/events`, `/ws`; CI with all gate scaffolding **including mutation testing and the forbidden-zone path protection**; forge VM provisioned. The architect writes or line-reviews essentially all of this — it is the DNA every worker inherits.
**Exit:** fresh VM → install → login → live events over WS; a deliberately gate-violating test PR is correctly rejected by CI. *(v0.1.0)*

### Phase 1 — Parallel tracks (3–5 weeks)
Five independent tracks, one worker stream each, integration forbidden until Phase 2:

- **A — Agent & protocol:** `vt-proto`, `vt-agent`, enrollment CA, metrics pipeline, conformance transcripts.
- **B — Proxmox adapter:** full adapter against recorded fixtures + nested PVE in the forge; no production PVE contact.
- **C — Voidwatch:** complete policy engine — mode ladder, risk classes, denylist, approvals, audit chain — as pure logic with the exhaustive mode×risk matrix. Parallelism makes *policy-before-AI* free; it lands before Odysseus exists.
- **D — App Vault:** manifest format, Tera rendering, plan/diff engine, the 5 launch manifests; testable standalone against fixtures.
- **E — UI foundation:** shell, auth flows, resource/device pages, generated API client, WS live updates.

**Exit:** each track's conformance suite green; **G4 hardware checkpoint:** real agents enrolled on the Ubuntu services VM + sessrumnir, one week of real uptime with reconnects. *(v0.2.0)*

### Phase 2 — Integration (3–5 weeks)
Serial-ish by nature; integrator-heavy. Docker adapter over agent relay; deploy pipeline end-to-end (render → plan → approve → apply → health gate) with Caddy; Odysseus on top of the already-hardened Voidwatch (providers, tool registry, sessions, SSE); approvals UI; **restic backup engine** (pulled forward per §0.1) wired to app manifests; AI eval suite + prompt-injection tests.
**Exit (all G4, on real hardware):** real Immich + Nextcloud redeployed through VoidTower in production; "install vaultwarden behind the proxy" succeeds end-to-end in Assisted mode; seeded injection cannot trigger a destructive action; backup → restore of a real app verified. *(v0.5.0 — the thesis release)*

### Phase 3 — Hardening & 1.0 (2–4 weeks)
Plugin host + two first-party plugins + SDK docs; 72 h forge soak (3 simulated agents, synthetic load, memory ceiling, zero event-log gaps); security pass per §10 (authz matrix, redaction, `vt audit verify`); upgrade-path tests; runbooks (install/backup/restore/update); bug triage; API freeze.
**Exit:** the two-week-vacation test. *(v1.0.0)*

**Total: ~2.5–4 months calendar.** If tracks finish early, pull T1 items (automation engine first) into Phase 3 as *new tracks with new specs* — never by letting an existing track wander past its spec. The full v0.1 vision surface remains out of scope for 1.0 regardless of spare agent capacity: T2 features are excluded because of their permanent maintenance and security surface, not their build cost.

---

## 17. Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| **Architect review saturation** (the new critical path) | Certain | Review tiering §15.5; gates make G4 short; milestone pacing sized to ~5–10 h/week; merge queue, not merge interrupts. |
| **Gate erosion by agents** (self-unblocking) | High in unattended runs | Forbidden zone #5, CODEOWNERS + CI path protection, snapshot-rollback of bad batches. |
| **Test gaming / false confidence** (tests that mirror the bug) | High | Mutation testing on critical crates; adversarial reviewer with clean context; acceptance tests named in the spec *before* implementation. |
| **Convention drift across parallel tracks** | High | Phase 0 spine written/reviewed by the human; per-crate CLAUDE.md; fresh sessions; integrator + workspace-wide gates; weekly integration checkpoint. |
| **Hardware-serial bottleneck** (one PVE cluster, one GTX 1080, one production dataset) | Certain | Nested PVE + fixtures cover 90% pre-G4; live checklist batched at phase exits; snapshot-before-apply on every live test. |
| **Token/subscription spend of 24/7 operation** | High | Burst model instead of perpetual; spend tracked per milestone; overnight batches only on fully-specified tasks (idle exploration is the expensive failure mode). |
| **Scope resurrection of the full v0.1 vision** ("agents are free, build everything") | High | Tier rule §0: T0 admission is gated on verification and operational surface, not build cost. Pull-forwards require a spec and an ADR. |
| Prompt injection / AI safety incident in the product | Medium | Voidwatch lands in Phase 1 before Odysseus exists; denylist; snapshot-before-apply; injection tests in CI. |
| SQLite ceiling | Low | Unchanged: measured home-scale load is trivial; `vt-store` trait is the seam. |
| Competing with Cognitio OS for the architect's attention | High | Unchanged and now sharper: agent teams multiply *implementation*, not *your* review hours — running two agent-driven megaprojects saturates you twice as fast. Pick a primary per quarter. |

---

## Appendix A — Traceability: v0.1 vision → this document

| v0.1 section | Disposition |
|---|---|
| AI First | §9, T0 (narrowed), policy-gated per §3.2 |
| Family Friendly | Personas §1.4 — P2 in T0, full family features T2 |
| One Home / Unified Object Model | §6 resources + relationships, T0 |
| Device Management | §4 agent, T0 (Linux), T1 (Win/mac), power/WoL T0 |
| Cluster Management | T2 (VoidMesh) |
| Proxmox Integration | §M2, T0 (single node + your two hosts; cluster features T1) |
| Container Platform | Docker/Podman T0, Compose T0, K8s T2 |
| Marketplace | App Vault T0 (5 apps, in-tree), remote marketplace T1 |
| AI Orchestrator multi-model | T1; two providers T0 |
| AI Skills | = tool registry (§9.2) + plugins (§8), T0 |
| Automation Engine | T1 |
| Storage Layer | Read/report T0; provisioning T1/T2 |
| Networking | Discovery T1; management T2 |
| Monitoring | §11, T0 |
| Gaming Platform | T2 (GPU-passthrough VM lifecycle T0 via Proxmox) |
| Security | §10, T0 |
| Family Features | RBAC T0; profiles/screen-time T2 |
| Plugin System | §8, T0 (local), marketplace T1 |
