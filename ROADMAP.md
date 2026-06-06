# VoidTower Roadmap

VoidTower is a self-hosted infrastructure command tower — control plane, app catalog, AI-ops integration, and automation in one local-first platform.

---

## Current State (what ships today)

Features confirmed present in the codebase (pages + API modules).

| Area | What exists |
|---|---|
| **Dashboard** | Customizable widgets — CPU/RAM/disk/network charts, container summary, alert count, clock, drag-to-reorder |
| **Services** | List systemd units, start/stop/restart/enable/disable, view logs, tag filtering |
| **Containers** | Docker list, start/stop/restart/remove, log viewer, exec shell, compose file editor with staged diff |
| **App Vault** | 40+ one-click deployments (see full list below), management panel with Containers/Compose/Logs tabs |
| **AI / Models** | Download GGUFs, pull Ollama models, import into llama.cpp; AI workspace iframe embed; AI-based app recommendations |
| **VMs** | KVM/QEMU via libvirt (`virsh`); Proxmox API integration — list/start/stop QEMU VMs and LXC containers |
| **Files** | Filesystem browser with Monaco editor, image viewer, PDF viewer, new file creation, download |
| **Terminal** | Full PTY browser terminal, shell auto-detection, SSH session manager |
| **Reverse Proxies** | nginx-backed proxy rule manager — domain/upstream/SSL/iframe-embed headers, validated and reloaded |
| **Firewall** | UFW rule manager — add/delete rules, enable/disable, allow/deny display |
| **WireGuard** | Peer manager — keypair generation, IP allocation, add/remove peers, client config |
| **Storage** | Block device tree, mount manager, fstab editor, format disks, SMART health, mdadm RAID status/creation, configurable storage paths |
| **Network** | Real-time interface stats, ARP/LAN neighbour table, bandwidth charts |
| **Backups** | Restic jobs — init, run, list snapshots, integrity check, dry-run restore test, confidence scoring |
| **Alerts** | Metric threshold alerts + TCP/HTTP status checks, ack/resolve, public `/status` page |
| **Automation** | Cron-style shell jobs, run history with output, enable/disable |
| **Secrets** | AES-256-GCM encrypted store, reveal-on-demand with audit logging |
| **Resource Tags** | Color-coded tags assignable to services/containers, page filtering |
| **Timeline** | Global audit timeline with category chips, search, outcome filter, paginated scroll |
| **Capabilities** | Detect installed tools (Docker, libvirt, WireGuard, restic, nginx, GPU) with version strings and install hints |
| **Diagnostics** | 12 system health checks — config/data dirs, DB, frontend assets, disk space, Docker daemon, nginx config, port bind |
| **Security** | Session list for all users, revoke individual/all-other sessions, full audit log |
| **Integrations** | Scoped API tokens; Odysseus config (enable/disable, MCP toggle, webhook secret); SSE event stream; webhook trigger; tool manifest at `/api/integrations/odysseus/manifest` |
| **Themes** | 7 built-in themes + live custom token editor (CSS variables), 14-param animation editor |
| **Animated Backgrounds** | 7 canvas presets (Void, Grid, Aurora, Pulse, Noise, Hex, Circuit) + 4 glass levels |
| **Updates** | In-UI updater — Docker image check/apply; bare-metal pull/rebuild with rollback points; OS package updates (apt/pacman/dnf) |
| **TOTP** | Backend `totp.rs` module present |
| **Multi-user / RBAC** | Owner / Admin / Operator / Viewer roles |

---

## Must-Have Before Public Release

From `future_plan.md` — 10 non-negotiable items for a credible first public release.

| # | Feature | Description | Size |
|---|---|---|---|
| 1 | **Plugin system** | Register API routes, UI panels, automation actions, alert providers, App Vault templates, Odysseus tools, and infrastructure backends; permissions declared; off by default | XL |
| 2 | **Capability detection page** | Already partially built — needs full per-capability show/install hints; currently shows detection but lacks "why it matters / how to enable" guidance | S |
| 3 | **Doctor / diagnostics mode** | `voidtower doctor` CLI command + JSON output; UI partially present at Settings → Diagnostics; needs CLI surface and JSON output | M |
| 4 | **Disaster recovery mode** | Full instance backup/restore, emergency admin reset, emergency disable AI/automations/webhooks, `voidtower export-config`/`import-config` CLI; must work without the web UI | L |
| 5 | **Dry-run / change planning** | Before-execution change plan UI for firewall changes, proxy changes, app deployments, backup restores, and Odysseus-triggered actions — showing files touched, services restarted, rollback availability | L |
| 6 | **Policy engine** | Define what each actor (user/automation/plugin/API token/Odysseus) may do on which resources, in which time window, with what approval requirement | XL |
| 7 | **Secrets manager** | Already built (AES-256-GCM store); gaps: secret rotation, scoped access per token, full redaction from Odysseus context bundles | M |
| 8 | **Resource tags** | Already built for services/containers; gaps: extend to VMs, backups, automations, alerts, App Vault deployments, API tokens; wire into policy and alert routing | M |
| 9 | **Global timeline** | Already built; gaps: link Odysseus actions to timeline entries, export selected range | S |
| 10 | **Backup verification** | Already has restore-test and check endpoints; gap: scheduled restore tests, backup confidence dashboard card, alert when never restore-tested | M |

---

## Phase 4 — Planned (from original spec, not yet built)

Items from `plan.md` Phase 4 that are absent or only skeleton-present in the codebase.

- [ ] **Visual automation workflow editor** — drag-and-drop trigger/action graph; current editor is YAML-only shell commands
- [ ] **OIDC / passkeys (WebAuthn)** — currently username/password + TOTP only
- [ ] **Plugin SDK** — no plugin architecture exists yet; everything is compiled into the monolith
- [ ] **WireGuard manager** — basic peer management exists; missing: peer stats, QR code export, interface creation from scratch
- [ ] **LXC management** — no LXC page exists; plan.md Phase 3 listed this but it was not implemented
- [ ] **Agent / multi-node mode** — plan.md Phase 3; no agent binary mode or multi-node dashboard present

---

## Feature Backlog (roughly prioritized)

### Infrastructure intelligence

- [ ] Config drift detection — detect when systemd units, Docker Compose files, firewall rules, or proxy configs change outside VoidTower; show expected vs actual diff with reconcile/accept options
- [ ] Inventory and asset database — lightweight CMDB: hardware, CPU, RAM, disk, GPU, OS/kernel versions, installed packages, owners, notes, warranty metadata per node
- [ ] Declarative state mode (GitOps) — export current state as YAML, import desired state, dry-run apply, optional Git sync

### Operations

- [ ] Maintenance windows — schedule windows, suppress alerts during them, allow selected automations only, notify before/after
- [ ] Incident mode — create incident from alert, attach logs/metrics/services, owner assignment, status tracking, postmortem export, Odysseus investigation handoff
- [ ] Notes per resource — Markdown notes pinnable on nodes, containers, VMs, services, alerts, automations, backups, security findings
- [ ] Snapshot-aware operations — create VM/Btrfs/ZFS/Compose rollback points before dangerous changes; UI to compare before/after and roll back
- [ ] Update management — controlled updates for VoidTower, App Vault templates, Docker images, OS packages, and (future) plugins with changelog, risk level, dry-run, rollback

### AI / Odysseus depth

- [ ] Policy engine for Odysseus actions (see must-have #6 above)
- [ ] "Send to Odysseus" buttons — on alerts, failed services, containers, VMs, backup failures, security findings, log selections; package context with secrets redacted
- [ ] AI approval queue UI — pending high-risk AI-requested actions; approve once / deny / approve with time limit / create policy from repeated safe action
- [ ] Full event stream subscriptions — currently SSE endpoint exists; needs: webhook outbound push, MCP resource/event support for agents

### Developer experience

- [ ] Public API SDKs — generate TypeScript, Python, Go SDKs from the existing OpenAPI spec
- [ ] OpenAPI documentation UI — expose `/api/openapi.json` with a browsable Swagger/Redoc UI
- [ ] Demo / simulation mode — `voidtower --demo` creates fake nodes/metrics/alerts/containers/VMs/backups/automation runs; does not touch real host

### App Vault expansion

- [ ] Custom app deployment form — "Deploy custom" button: image, name, port map, volume map, env vars → generates and saves compose file; no YAML knowledge needed
- [ ] AI integration badges — per-app badge tier: AI Native / AI Aware / AI Ready / none; defined in YAML catalog; shown as colored chip on app card

### VM / Android hosting

- [ ] GPU passthrough UI — assign a physical GPU to a KVM VM with a toggle (UI only; libvirt XML generation)
- [ ] ISO library browser — upload or link ISOs for new VM creation
- [ ] VM snapshot management UI — Proxmox and libvirt snapshot create/restore/delete
- [ ] Waydroid instance manager — manage Waydroid Android containers, expose via browser stream (scrcpy → WebRTC)
- [ ] Android-x86 in QEMU — isolated Android VMs for testing
- [ ] Redroid support — Docker-based multi-instance Android with ADB / scrcpy stream embed

---

## App Vault — Planned Apps

Apps mentioned in `future_plan.md` section 21 that are **not yet** in `app-vault/apps/`:

| App | Category | Notes |
|---|---|---|
| Matrix / Synapse | Communication | Self-hosted encrypted chat |
| Jitsi | Communication | Self-hosted video calls |
| SimpleX | Communication | No phone number required |
| Ntfy | Notifications | Push notifications to own devices |
| Drone CI / Woodpecker CI | Dev | CI/CD pipelines |
| Private Docker Registry | Dev | Mirror of `registry:2` |
| Zigbee2MQTT | Home automation | Direct Zigbee control without cloud |
| Node-RED | Home automation | Already in original plan but not in vault |
| Cloudflare Tunnel | Network | Expose services without port forwarding |
| Technitium DNS | Network | Full DNS server with web UI |
| Baserow / NocoDB | Productivity | Self-hosted Airtable |
| Plane | Productivity | Project management |
| EmulatorJS + ROM library | Entertainment | Browser-based retro gaming |
| LocalAI | AI | OpenAI-compatible API over local models |
| Stable Diffusion / ComfyUI | AI | Already present as `comfyui.yml` — Stable Diffusion WebUI missing |
| Whisper | AI | Local speech-to-text |

Apps already in `app-vault/apps/` (40 present): adguardhome, authentik, changedetection, code-server, comfyui, dozzle, freshrss, gitea, grafana, homeassistant, immich, jellyfin, jitsi, kavita, llama-cpp, matrix-synapse, mealie, minio, n8n, navidrome, nextcloud, nginx-proxy, odysseus, ollama, opencloud, open-webui, outline, paperless, pihole, portainer, redroid, searxng, stirling-pdf, syncthing, tailscale, uptime-kuma, vaultwarden, vikunja, wireguard-easy, youkidex.

---

## Odysseus Integration Gaps

What the spec requires vs what `backend/src/api/integrations.rs` actually implements.

| Feature | Status |
|---|---|
| Scoped API token creation | Implemented |
| Odysseus config UI (enable/disable, MCP toggle, webhook secret) | Implemented |
| Tool manifest at `/api/integrations/odysseus/manifest` | Implemented |
| SSE event stream (`/api/integrations/events`) | Implemented |
| Webhook bridge (inbound, HMAC-signed, triggers automations) | Implemented |
| Emergency disable all AI access | Implemented |
| MCP server (built-in, tool-serving) | Config flag exists (`mcp_enabled`) — no actual MCP server implementation |
| "Send to Odysseus" buttons in UI | Not implemented — plan.md lists this as optional future |
| AI approval queue / pending action UI | Not implemented |
| Event stream webhook outbound push | Not implemented — SSE only |
| Per-Odysseus-action policy enforcement | Not implemented — policy engine not built |
| Odysseus integration events linked to timeline | Partial — audit log exists; no explicit Odysseus tagging |
| Voidwatch toolpack definitions | Present in `voidwatch/toolpacks/` (20 packs) — see README |

---

## Known Issues / Tech Debt

- **Pi-hole pinned to v5** (`2024.07.0`) — Pi-hole v6 changed config format; app vault template needs update
- **Odysseus/Ollama dual-deploy port conflict** — Odysseus on 7000, Ollama on 11434; no guard in the installer when both are present
- **YoukiDex APK sideload not automated** — `youkidex.yml` is in App Vault but the APK install step is manual
- **App Vault embed steps not complete** — auto-proxy creation with header stripping (X-Frame-Options removal) for iframed apps is partially wired but not automatic for all apps
- **Odysseus `voidlink-latest` Docker image CI workflow** — no GitHub Actions workflow builds or publishes `aio-latest` from this repo; image must be built manually
- **TrueNAS AIO end-to-end test pending** — pool name in docs defaults to `tank` but actual user pool is `main`; deploy/truenas YAML needs pool-name verification pass
- **MCP server is a stub** — `mcp_enabled` setting exists and is toggleable but no MCP protocol server is actually implemented
- **LXC management missing** — was Phase 3 in plan.md; no LXC page or backend module exists
- **Agent/multi-node mode missing** — was Phase 3 in plan.md; `--agent` flag in installer but no agent mode in the binary
- **TOTP** — `totp.rs` exists in backend but no UI page or enrollment flow was found in `frontend/src/pages/`

---

## Not Planned

- **More themes** — Odysseus ROADMAP: "I prob shouldnt add more themes"; same applies to VoidTower
- **iPhone / iOS VM** — no legal option exists; Corellium is paid/enterprise only; not in scope
- **macOS VM** — OSX-KVM is legal grey area; not in scope for mainline
- **Cloud dependency / telemetry / license server** — never; out of scope by principle
- **Kubernetes / etcd / external consensus** — plan.md explicitly excluded for MVP; not planned
