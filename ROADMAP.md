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

## Phase 4 — Planned (from original spec)

| Item | Status |
|---|---|
| Visual automation workflow editor | Not started — cron+shell only |
| OIDC / passkeys (WebAuthn) | Not started — username/password + TOTP only |
| Plugin system / SDK | **Done** (`f38e640`) — zip install, iframe host, dynamic sidebar, plugin.json manifest |
| WireGuard manager | Partial — peer add/remove/list; missing peer stats, QR export |
| LXC management page | **Done** (`418963f`) — list/start/stop/shutdown/restart via `pct`; capability-gated nav item |
| Agent / multi-node mode | Not started — `--agent` flag parsed but mode not implemented |

---

## Phase 5 — AI Living Desktop

These items transform VoidTower from an admin panel into a true local-first AI operating system. They build on the existing Void Mode shell and Odysseus voidlink integration.

### Bug Fixes (blocking polish)

- [ ] **Fix "Open WebUI in VoidTower" button** — App Vault deploy creates a proxy entry but routing is wrong for apps with `web_path` (e.g. Pi-hole `/admin`) or non-standard `web_port`. Fix: auto-creation must pull `web_port`/`web_path` from YAML catalog and strip iframe-blocking headers on the generated proxy rule.
- [x] **Fix theme randomizer** — Done (`1d638a3`): `--text-disabled` + all 5 `*-subtle` rgba tokens now generated and cleared on reset.
- [x] **Fix proxy edit/delete** — Done: inline edit form (dry-run plan + save) and delete with confirmation are both present.

### Full UI Customization

- [ ] **Instance branding** — Settings → Appearance → Branding: instance name (replaces "VoidTower" wordmark), logo upload (PNG/SVG, becomes favicon and sidebar icon), login page background image, login tagline, custom CSS injection field. All client-side from backend-stored settings.
- [ ] **Navigation editor** — Settings → Navigation: toggle individual nav items on/off, rename them, pick a custom Lucide icon, reorder via drag-and-drop, create or rename nav groups. Changes are per-user and stored in their profile. Owner can set instance-wide defaults.
- [ ] **Menu position & layout** — choose navigation placement: left sidebar (Tower Mode default), right sidebar, top horizontal bar, bottom bar, floating pill (Void Mode dock default), or pop-out drawer (hidden, slides in on hover or shortcut). Open/close animations are configurable (slide, fade, spring). Auto-hide-on-scroll toggle.
- [ ] **Custom tabs** — Settings → My Tabs: add personal nav entries with name, icon, and a target: any VoidTower page, an embed URL (iframe), a local file path opened in the file viewer, or a WebSocket stream. Useful for personal Grafana dashboards, Netdata, Homer, Jupyter notebooks, documentation, internal tools.

### User Management & Multi-tenancy

- [ ] **Full user management page** — Settings → Users: beyond current RBAC, add per-user profile (avatar, display name, email), per-user nav layout, per-user default workspace and theme, per-user AI endpoint (URL, API key, model, system prompt). Includes a household member onboarding wizard that creates an account and assigns a starter layout preset.
- [ ] **Per-user AI endpoint** — each user configures their own Odysseus URL, API key, and model. Owner sets a shared fallback. Supports family members with different AI preferences or spending limits.
- [ ] **User groups** — group users (e.g. "Family", "Admins", "Guests"), assign group-level resource permissions, view per-group activity.
- [ ] **Guest / read-only access** — shareable URL granting limited time-limited read access (view metrics, alerts status) without a login; configurable scope (pages visible) and expiry.

### AI Agent Visualization

- [ ] **Live agent canvas** — an animated overlay layer between the AnimatedBackground and the panel canvas that renders connected AI agents and running automations as small animated characters. Characters move between resource "nodes" based on the SSE event stream. Off by default; toggle in Settings → Appearance → Agent Visualization.
- [ ] **Agent speech bubbles** — small floating labels near characters showing their current action ("analyzing logs", "restarting container"). Driven by SSE events, fade in and out automatically, max ~40 chars.
- [ ] **Agent inspector** — click an agent character to open a small popover: agent name, current task, recent actions, resource being acted on, "pause this agent" toggle.
- [ ] **Configurable agents** — Settings → Agent Visualization: choose which agents/automations appear, their avatar style (dot, sprite, emoji), and whether path trails are shown.

### More Background Animations

- [ ] **New canvas presets**: Particles (drifting dots with connecting lines), Matrix Rain (falling green characters), Starfield (parallax depth field), Neural Network (pulsing nodes and weighted edges), Ocean (dark slow waves), Nebula (soft slow-moving color clouds).
- [ ] **Metric-reactive mode** — optional: animation speed and intensity respond to live system metrics (CPU load → pulse speed, network traffic → particle density). Driven by the same WebSocket metric stream as the status bar.
- [ ] **Agent-reactive mode** — when agent visualization is active, the background canvas shows subtle ripples or trails emanating from positions where agents are active.

### Proxy Management — Full Nginx Capabilities

- [ ] **Proxy edit form** — full edit UI for existing rules: upstream URL, domain, SSL, custom request/response headers, frame policy, rate limit, auth headers, cache settings.
- [ ] **Proxy presets** — one-click configuration presets: "Strip iframe blockers" (remove X-Frame-Options, loosen CSP), "Add basic auth", "Rate limit 10 req/min", "Force HTTPS redirect", "WebSocket passthrough", "Gzip + cache static assets".
- [ ] **AI proxy recommendations** — when a proxy is created for an App Vault app, VoidTower checks the YAML catalog for known embed requirements and auto-suggests the right preset (Grafana needs CSP relaxed, Portainer needs WebSocket passthrough, etc.).
- [ ] **Proxy health dashboard** — list all rules with upstream reachability (green/amber/red), last check timestamp, and response time. Manual "test" button per entry.
- [ ] **Let's Encrypt integration** — SSL certificate request and auto-renewal via Certbot/acme.sh as a toggle in the proxy edit form; renewal status shown in the proxy list.
- [ ] **Wildcard subdomain routing** — configure `*.home.domain.tld` to auto-route to apps by name from a single wildcard proxy rule.

### Search Expansion

- [ ] **Full-text search** — Ctrl+K searches across: containers (name/image), services (unit name), timeline events (description), secrets (name only, never value), automation jobs, file names in Files, tags, and notes. Results grouped by type with icons.
- [ ] **Search filters** — filter chips in command bar results: resource type, date range, status, tag. Fully keyboard-navigable.
- [ ] **Saved search shortcuts** — save a query as a named shortcut; invoke with `!name` in command bar.
- [ ] **Inline Odysseus search** — `/query` prefix in command bar sends query to Odysseus and shows the AI response as a result card inline before opening the Odysseus panel.

### AI Creative Studio (Odysseus Voidlink Deep Integration)

Full integration of the Odysseus voidlink automation and generation capabilities into the VoidTower UI. VoidTower becomes a local hub for AI-driven content creation, not just infrastructure management.

- [ ] **AI Studio page** — top-level page combining all local AI generation capabilities: image, video, TTS, STT, 3D. Shows available models, their loaded/unloaded state, VRAM usage, and queue depth. Accessible as a Void Mode panel.
- [ ] **Image generation panel** — prompt → ComfyUI or Stable Diffusion WebUI via API; built-in image viewer with generation metadata; prompt history; workflow picker (txt2img, img2img, inpainting, upscale); gallery of recent outputs.
- [ ] **Video generation panel** — text/image prompt → local video diffusion (AnimateDiff, SVD, CogVideoX via ComfyUI); built-in video player; generation queue; preview frames while generating.
- [ ] **TTS panel** — local TTS via Kokoro, Coqui, or Piper; voice picker; built-in audio player; save to Files; voice cloning workflow (upload a 10s clip → fine-tune).
- [ ] **STT panel** — local Whisper transcription; upload audio/video or record from browser microphone; output plain text or SRT; send to Odysseus for summarization.
- [ ] **Content pipeline editor** — visual node graph for AI content pipelines: e.g. "text prompt → image → animate → TTS narration → save to Files". Nodes represent local AI tools. Built on the existing Automation backend with new AI node types.
- [ ] **3D generation panel** — local 3D model generation (TripoSR, Zero123++, Shap-E); built-in Three.js viewer (orbit, zoom, wireframe, export GLB/OBJ); generate from image or text prompt.
- [ ] **MCP tool panel** — inspect and invoke any MCP tool registered in Odysseus's tool manifest directly from VoidTower. Input form auto-generated from tool schema. Useful for testing and debugging without a terminal.
- [ ] **Local agent terminal panel** — Void Mode panel that runs a local AI coding agent (Claude Code, Aider, or any CLI agent) inside a PTY with context buttons: "Add this container's logs", "Add this file from Files". Wires into existing Terminal + Files infrastructure.
- [ ] **Inbuilt media viewers** — first-class Void Mode panel types for: image gallery, video player, audio player, PDF reader, 3D model viewer, notebook viewer (`.ipynb`). Any file in Files or AI-generated output opens in the correct viewer panel.
- [ ] **Automation library** — pre-built automation templates: "Daily blog post draft", "Auto-caption video", "Generate product images from description", "Transcribe and summarize meeting recording". One-click install from a curated library, customizable via Odysseus.

---

### Void Mode — /ask Chat Popup (Odysseus Quick Chat)

Replace the current `/ask` UX (which opens the full Odysseus iframe panel) with a lightweight inline chat overlay that streams responses without leaving the current context. The backend acts as a **full transparent reverse proxy** to Odysseus — not a thin wrapper — so every Odysseus feature (tools, memory, RAG, MCP, multi-turn) is available through the popup and through VoidTower's auth wall.

**Backend: Wildcard Odysseus Reverse Proxy**

- [ ] **`/api/odysseus/*` wildcard reverse proxy** — A single Axum wildcard route forwards all methods (GET/POST/DELETE/etc.) verbatim to the configured Odysseus URL (`http://localhost:7000/{rest}`). Request body, headers, and query params forwarded as-is. Response body streamed back (chunked transfer / SSE passthrough). CORS and auth handled by VoidTower; Odysseus URL and API key never reach the browser.
- [ ] **System context injection** — For POST requests to `/api/odysseus/v1/chat/completions`, the backend intercepts the JSON body, prepends a VoidTower context message (instance state, running resource counts, focused panel) to the `messages` array, then forwards. All other routes pass through unmodified.
- [ ] **Auth gate** — Wildcard route requires a valid VoidTower session cookie or API token. Unauthorized requests return 401 before touching Odysseus.
- [ ] **Graceful degradation** — If Odysseus is unreachable, returns `{ error: "odysseus_unavailable" }` with 502. Frontend shows a "Odysseus offline" state rather than hanging.

**Frontend: Chat Popup**

- [ ] **Animated chat popup** — `/ask query` or dedicated keyboard shortcut → compact floating panel (400×320px) spring-animates up from the dock. Appears above all panels. Dismiss with Escape or click-outside.
- [ ] **Full Odysseus client** — The popup sends requests to `/api/odysseus/v1/chat/completions` with streaming. Because it's a transparent proxy, tool calls, memory reads, MCP tool invocations, and RAG queries all work exactly as they do in the full Odysseus UI.
- [ ] **Conversational thread** — Scrollable message history maintained per Void Mode session. Cleared on page reload or manual clear button.
- [ ] **Context injection** — Focused panel title and component key are included in the system message. Paperclip button pins current panel's live data as additional context.
- [ ] **Inline rendering** — Responses rendered with lightweight markdown: bold, inline code, fenced code blocks with copy button. Tool call results shown as collapsible blocks.
- [ ] **"Open in Odysseus panel" button** — Sends the current conversation history to the full Odysseus iframe panel via postMessage.
- [ ] **Slash commands in command bar** — `/ask <query>` pre-fills the popup. `/clear` wipes history. `/copy` copies last response.

---

### Odysseus Self-Knowledge — Codebase Context at Launch

When Odysseus loads in VoidTower, inject a structured knowledge bundle so the AI knows exactly how VoidTower and Odysseus-Voidlink are built. Ask "how do I add a new background animation?" and get a working template, not a guess.

**What gets injected (the Boot Context Bundle):**

- [ ] **Architecture overview** — component tree (Backend Rust → SQLite → API → Frontend React/Vite → Void Mode AIOS), key file paths, module responsibilities
- [ ] **Extension templates** — ready-to-paste code templates for:
  - Adding a new canvas background animation (AnimatedBackground canvas preset + params)
  - Deploying a new MCP tool with Docker (App Vault YAML + `docker-compose.yml` + VoidTower tool manifest entry)
  - Creating a new API-backed App Vault entry (YAML schema, required fields, env var conventions)
  - Adding a new native Void Mode panel (NativePanelShell pattern, PANEL_REGISTRY entry, fetch pattern)
  - Writing a new backend API module (Rust axum router pattern, SQLite migration pattern)
  - Registering a new Odysseus tool in the manifest (schema, handler, permissions)
- [ ] **API surface reference** — all `/api/*` routes with method, request body shape, and response shape (generated from the OpenAPI spec or parsed from `client.ts`)
- [ ] **Current instance state** — live snapshot appended at session start: running services count, active containers, alert count, VoidTower version, enabled capabilities (Docker/libvirt/WireGuard/GPU/etc.)
- [ ] **MCP tools in scope** — list of all currently registered Odysseus tools with their descriptions, so Odysseus can recommend the right tool for a given task

**Implementation:**

- [ ] **`/api/ai/context` endpoint** — returns the full boot context bundle as structured JSON. Includes static architecture docs + live instance snapshot. Called once when the Odysseus panel opens.
- [ ] **Context injection via URL param** — VoidTower passes `?vtx=<base64-encoded-context-summary>` when opening the Odysseus iframe; Odysseus reads this on load and prepends to its system prompt.
- [ ] **Knowledge file storage** — `data/ai-context/` directory: `architecture.md`, `templates/*.md`, `api-reference.md`. Editable from the VoidTower Files panel. Odysseus reads these at startup via the MCP filesystem tool.
- [ ] **Auto-regenerate on update** — when VoidTower updates (version bump), the context bundle is regenerated and saved; stale context is flagged with a "knowledge outdated" warning in the Odysseus panel header.
- [ ] **`/api/ai/context/templates`** — list and serve individual template files; allows Odysseus to `GET /api/ai/context/templates/new-animation` and receive the full template text directly via MCP tool call.

---

### Proxmox Full Management Suite

Full Proxmox Virtual Environment management built into VoidTower — on par with PVEDiscord/PegaProx for depth, but native to the AIOS panel system. No separate web UI needed.

**Proxmox Tower Mode pages (sub-navigation under VMs):**

- [ ] **Nodes overview** — all Proxmox cluster nodes with CPU/RAM/storage utilisation bars, uptime, kernel version, subscription status badge. Click a node to filter all other views to that node.
- [ ] **QEMU VMs** — full table: vmid, name, status (running/stopped/paused), CPU %, RAM %, disk, network I/O, node, tags. Actions per row: start / stop / shutdown (graceful) / reset / suspend / resume / migrate. Bulk select + action. Status live-polls every 5 s.
- [ ] **LXC Containers** — same table + actions as VMs. Separate page because LXC has different fields (unprivileged flag, rootfs vs disk, ostype). Start / stop / restart / shutdown / migrate.
- [ ] **Console access** — noVNC WebSocket proxy: VoidTower backend opens a WebSocket tunnel to the Proxmox VNC console and forwards it to the browser. Works for both QEMU (VNC/SPICE) and LXC (xterm.js over serial console). No Proxmox web UI needed.
- [ ] **VM creation wizard** — multi-step form: choose node, ISO from library, resource allocation (cores, RAM, disk size, storage pool), network bridge, optional cloud-init config. Generates API call to `POST /nodes/{node}/qemu`. LXC version uses templates from Proxmox template library.
- [ ] **Snapshot management** — list snapshots per VM/LXC with name, description, date, RAM state included. Create snapshot (with description), rollback with confirmation, delete. Dry-run enabled: show what would be reverted before committing.
- [ ] **Storage browser** — Proxmox storage pools: list content (ISOs, templates, VM disks, snippets), upload ISOs from browser, delete content, view pool usage. Links to VoidTower's existing Storage page for local block devices.
- [ ] **Backup jobs** — list all vzdump backup jobs (scheduled and manual). View job history, restore a backup to a new VM/LXC with a wizard. Integrates with Proxmox Backup Server (PBS) if configured.
- [ ] **Cluster / node tasks** — running and recent task list from `/nodes/{node}/tasks`. Shows progress bars for long-running ops (migration, backup, restore). Subscription status and cluster quorum health.
- [ ] **Network & firewall** — view Proxmox SDN/bridge network config and firewall rules per node and per VM. Edit bridge assignments and VLAN tags. (Full edit is phase 2; read + assignment in phase 1.)

**Void Mode native panel:**

- [ ] **NativeProxmoxPanel** — compact panel with three tabs: VMs (rows with status dot, name, CPU bar, memory bar, start/stop button), LXC (same), Snapshots (last 5 per selected VM). Clicking a row opens a full Tower Mode Proxmox VM page. Console button opens a new panel with the noVNC stream.

**Multi-host:**

- [ ] **Multiple Proxmox hosts** — Settings → Integrations → Proxmox: add multiple PVE hosts (URL, API token, optional TLS fingerprint). Each host shows as a collapsible group in the VMs page. VoidTower's existing Proxmox config struct (`ProxmoxConfig` in `api/types.ts`) extended with array support.

**Proxmox Backup Server (PBS) integration:**

- [ ] **PBS datastore browser** — list datastores, namespaces, and backup snapshots. Restore selected backup to a Proxmox VM. Show backup size, verify status (green tick / red X), and last GC run.

**Safety:**

- All destructive operations (stop, reset, rollback, delete) show the existing `ChangePlanModal` dry-run preview before executing.
- VM/LXC management actions are gated by VoidTower RBAC: Operator can start/stop; Admin can create/delete/snapshot; Owner can configure clusters.

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
- **MCP server** — ~~stub~~ Done (`3a23ed3`) — full SSE+message MCP server with 13 tools
- **LXC management** — ~~missing~~ Done (`418963f`) — `/lxc` page + `pct` backend
- **Agent/multi-node mode missing** — was Phase 3 in plan.md; `--agent` flag in installer but no agent mode in the binary
- **TOTP** — ~~`totp.rs` exists but no UI~~ Done (`16b3a59`) — Security page + login step

---

## Not Planned

- **More themes** — Odysseus ROADMAP: "I prob shouldnt add more themes"; same applies to VoidTower
- **iPhone / iOS VM** — no legal option exists; Corellium is paid/enterprise only; not in scope
- **macOS VM** — OSX-KVM is legal grey area; not in scope for mainline
- **Cloud dependency / telemetry / license server** — never; out of scope by principle
- **Kubernetes / etcd / external consensus** — plan.md explicitly excluded for MVP; not planned
