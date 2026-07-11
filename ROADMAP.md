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
| **App Vault** | 52 one-click deployments (see full list below), management panel with Containers/Compose/Logs tabs, custom-deploy form for arbitrary images |
| **Media Automation** | Full Servarr stack in App Vault — Sonarr, Radarr, Lidarr, Readarr, Prowlarr, Bazarr, Seerr, qBittorrent, Gluetun, Recyclarr, FlareSolverr |
| **AI / Models** | Download GGUFs, pull Ollama models, import into llama.cpp; AI workspace iframe embed; AI-based app recommendations |
| **AI Studio** | Image generation, TTS, STT, output gallery, MCP tool inspector/invoker with auto-generated forms — Tower Mode page + Void Mode native panel |
| **VMs** | KVM/QEMU via libvirt (`virsh`); Proxmox API integration — list/start/stop/reboot/reset/suspend/resume QEMU VMs and LXC containers, storage pool browser (list/upload/delete content), physical disk management (SMART, wipe, passthrough to VM), PBS backup tab, noVNC console, snapshots (create/rollback/delete with change-plan), VM state alerts, resource tags, AIOS native panel |
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
| **Themes** | 23 built-in themes (`frontend/src/theme/themes.ts`) + live custom token editor (CSS variables), 14-param animation editor |
| **Animated Backgrounds** | 8 canvas presets (Void, Grid, Aurora, Pulse, Noise, Hex, Hex Classic, Circuit) + 4 glass levels |
| **Updates** | In-UI updater — Docker image check/apply; bare-metal pull/rebuild with rollback points; OS package updates (apt/pacman/dnf) |
| **TOTP / SSO** | TOTP enrollment + login step (Security page); Authentik OIDC SSO login with group→role mapping and auto-create-on-login (Settings → Security); per-proxy Authentik forward-auth gating |
| **Multi-user / RBAC** | Owner / Admin / Operator / Viewer roles |

---

## Must-Have Before Public Release

From `future_plan.md` — 10 non-negotiable items for a credible first public release. CLAUDE.md's own maintained snapshot marks all 10 **Done**; verified directly against source below rather than re-trusting that table blind. The "Size" column is now moot (all shipped) but kept for history.

| # | Feature | Status | Remaining gaps |
|---|---|---|---|
| 1 | **Plugin system** | **Done** (`f38e640`) | zip install, iframe host, dynamic sidebar, `plugin.json` manifest — registers API routes/UI panels/automation actions |
| 2 | **Capability detection page** | **Done** | per-capability detection with install hints; confirmed present at Settings → Capabilities |
| 3 | **Doctor / diagnostics mode** | **Done** | `--doctor --json` CLI flag (per CLAUDE.md Phase-3 table) + UI at Settings → Diagnostics, 12 health checks |
| 4 | **Disaster recovery mode** | **Done** | confirmed: `backend/src/api/disaster.rs` has `export_config`/`import_config`/`emergency_reset_admin`/`emergency_disable` plus CLI-only `cli_export`/`cli_import` that work without booting the web server |
| 5 | **Dry-run / change planning** | **Done** | `ChangePlanModal` pattern wired into proxy create/edit, firewall rules, OS updates, container remove, backup delete, Proxmox stop/snapshot/rollback |
| 6 | **Policy engine** | **Done**, hardened in P0 | `backend/src/policy.rs` + `backend/src/voidwatch/` — actor/action/resource/tag matching, DB-backed rules, CRUD API + UI. `voidwatch::evaluate()` is now the single choke point for MCP `tools/call`, Studio `mcp_invoke`, and automation/webhook actions (P0.1); `api_token`/`automation`/`ai` actors are default-deny with a migrated allowlist (P0.2); mode ladder + risk classes gate the AI/automation ingress path (P0.3 backend); a hardcoded irreversibility denylist blocks 9 action classes regardless of mode (P0.4); bearer tokens carry real enforced scopes, closing the god-token bypass (P0.6). Still open: making mode-ladder verdicts mandatory (not advisory) at the six UI-driven handlers — needs an approvals-queue mechanism, tracked as [issue #11](https://github.com/niwlekakan/voidtower/issues/11) |
| 7 | **Secrets manager** | **Done**, with gaps | AES-256-GCM store, reveal-on-demand audit logging built; redaction on the AI context path shipped (P0.5: MCP tool-call output, Studio `mcp_invoke`, `get_context` bundle — `backend/src/api/redact.rs`); still missing: secret rotation, scoped per-token access |
| 8 | **Resource tags** | **Done**, with gaps | covers services/containers/VMs/backups/apps/proxmox_vm today (`GET /api/tags/map?type=`); still missing: automations, alerts, API tokens; not yet wired into policy/alert routing |
| 9 | **Global timeline** | **Done**, with gaps | global audit timeline with category chips/search/outcome filter; still missing: explicit Odysseus-action tagging, export selected range |
| 10 | **Backup verification** | **Done**, with gaps | restore-test, integrity check, confidence scoring all built; still missing: scheduled (cron-driven) restore tests and an alert when a backup has never been restore-tested |

---

## Phase 4 — Planned (from original spec)

| Item | Status |
|---|---|
| Visual automation workflow editor | Not started — cron+shell only |
| OIDC login | **Done** — `backend/src/oidc.rs` (PKCE flow, role-claim mapping) + `oidc_status`/`oidc_login`/`oidc_callback` in `api/auth.rs`; `users.auth_source`/`oidc_subject` columns; configurable in Settings → Security. Roadmap previously listed this as "Not started" — it isn't. |
| Passkeys (WebAuthn) | Not started — no `webauthn`/passkey code anywhere in the tree |
| Plugin system / SDK | **Done** (`f38e640`) — zip install, iframe host, dynamic sidebar, plugin.json manifest |
| WireGuard manager | Partial — peer add/remove/list; missing peer stats, QR export |
| LXC management page | **Done** (`418963f`) — list/start/stop/shutdown/restart via `pct`; capability-gated nav item |
| Agent / multi-node mode | Not started — `--agent` flag parsed but mode not implemented |

---

## Phase 5 — AI Living Desktop

These items transform VoidTower from an admin panel into a true local-first AI operating system. They build on the existing Void Mode shell and Odysseus voidlink integration.

### Bug Fixes (blocking polish)

- [x] **Fix "Open WebUI in VoidTower" button** — Done: the frontend already resolved `web_port`/`web_path` from the catalog and `write_nginx_port_conf` already stripped `X-Frame-Options`/CSP. The actual bug was that `open_ui` only wrote the nginx conf on first creation — once a `proxy_configs` row existed for a project, its embed port/upstream were never refreshed, so apps whose catalog `web_port` changed after first open (or that were opened before `web_port`/`web_path` existed) kept routing to the stale port forever. `open_ui` now always re-checks and rewrites the upstream/conf to match the current resolved port.
- [x] **Fix theme randomizer** — Done (`1d638a3`): `--text-disabled` + all 5 `*-subtle` rgba tokens now generated and cleared on reset.
- [x] **Fix proxy edit/delete** — Done: inline edit form (dry-run plan + save) and delete with confirmation are both present.

### Full UI Customization

- [x] **Instance branding** — Done: Settings → Appearance → Branding (instance name, logo upload, login background, tagline, custom CSS) plus sidebar/favicon now use the uploaded logo instead of the hardcoded Shield icon.
- [x] **Navigation editor** — Done: Customization → Navigation tab (toggle visibility, rename, custom Lucide icon per item, drag-reorder items/groups, create/delete groups, drag items between groups). Per-user via `user_nav_config` table, synced through `/api/nav-config`; owner can set the instance-wide default via `/api/nav-config/default`.
- [x] **Menu position & layout** — Done for left/right/top/bottom placement + auto-hide-on-scroll (Customization → Navigation → Layout). Top/bottom render as a single merged bar (nav group dropdowns + search/GPU/tag/status/bell, no redundant second bar) and slide+fade in when placement changes. Not done: floating pill and pop-out drawer placements.
- [x] **Custom tabs** — Done for Tower Mode: Customization → My Tabs (personal, per-user — title, icon, iframe or markdown target), surfaced in the sidebar/nav bar and viewable at `/tabs/:id`. Not done: page-link, local-file, and WebSocket target kinds (backend `kind` enum only supports iframe/markdown/builtin).

### User Management & Multi-tenancy

**Identity and MFA are intentionally not VoidTower-native here — they route through the existing Authentik integration.** `backend/src/api/auth.rs` already implements OIDC SSO login, Authentik-group → VoidTower-role mapping, and auto-create-on-first-login; `backend/src/api/proxy.rs` already gates individual proxies behind Authentik's forward-auth outpost. Native TOTP remains the MFA path only for local (non-SSO) accounts. So the items below should be framed as *extending/consuming Authentik* — richer group mapping, Authentik-backed guest flows — rather than building a second, parallel identity system. Confirmed still fully unbuilt on the VoidTower side: the `users` table (`backend/src/db/mod.rs`) has no `avatar`/`display_name`/`email` columns — only `id`/`username`/`password_hash`/`role`/`force_password_change`/`totp_*`/`auth_source`/`oidc_subject`.

- [ ] **Full user management page** — Settings → Users: beyond current RBAC, add per-user profile (avatar, display name, email — these are VoidTower-local fields independent of Authentik), per-user nav layout, per-user default workspace and theme, per-user AI endpoint (URL, API key, model, system prompt). Includes a household member onboarding wizard that creates an account and assigns a starter layout preset. For Authentik-backed users, profile fields (display name, email, avatar) should sync from the OIDC claims already received at login rather than being re-entered.
- [ ] **Per-user AI endpoint** — each user configures their own Odysseus URL, API key, and model. Owner sets a shared fallback. Supports family members with different AI preferences or spending limits.
- [ ] **User groups** — rather than building a separate VoidTower group table, map Authentik groups directly: the existing OIDC role-mapping config (`auth.rs`, Settings → Security) already converts one Authentik group → one VoidTower role; extending it to also tag the user with the Authentik group name(s) as metadata (not just the derived role) gives "Family"/"Admins"/"Guests"-style grouping for free, and the existing policy engine's actor matching (`backend/src/policy.rs`, already matches by actor/action/resource/tag) can target those group tags directly instead of inventing a second permission model. Local (non-SSO) accounts would still need a lightweight VoidTower-native group fallback.
- [ ] **Guest / read-only access** — shareable URL granting limited time-limited read access (view metrics, alerts status) without a login. Two viable paths: (a) a VoidTower-native time-limited share token (new, simplest), or (b) an Authentik "guest" flow/group with a short-lived OIDC session — more consistent with the identity model above if Authentik is already deployed, but adds a hard dependency on Authentik being present for a feature that should work standalone too.

### AI Agent Visualization

- [ ] **Live agent canvas** — an animated overlay layer between the AnimatedBackground and the panel canvas that renders connected AI agents and running automations as small animated characters. Characters move between resource "nodes" based on the SSE event stream. Off by default; toggle in Settings → Appearance → Agent Visualization.
- [ ] **Agent speech bubbles** — small floating labels near characters showing their current action ("analyzing logs", "restarting container"). Driven by SSE events, fade in and out automatically, max ~40 chars.
- [ ] **Agent inspector** — click an agent character to open a small popover: agent name, current task, recent actions, resource being acted on, "pause this agent" toggle.
- [ ] **Configurable agents** — Settings → Agent Visualization: choose which agents/automations appear, their avatar style (dot, sprite, emoji), and whether path trails are shown.

Confirmed not started — no `AgentCanvas` or agent-visualization code anywhere in `frontend/src`. Worth sequencing after the "MCP tool-call audit trail" backlog item above: once `mcp::invoke_tool` calls write structured `audit::log` entries with resource type/id, the agent canvas and inspector popover have a real per-action data source to render instead of needing new event plumbing.

### More Background Animations

- [ ] **New canvas presets**: Particles (drifting dots with connecting lines), Matrix Rain (falling green characters), Starfield (parallax depth field), Neural Network (pulsing nodes and weighted edges), Ocean (dark slow waves), Nebula (soft slow-moving color clouds).
- [ ] **Metric-reactive mode** — optional: animation speed and intensity respond to live system metrics (CPU load → pulse speed, network traffic → particle density). Driven by the same WebSocket metric stream as the status bar.
- [ ] **Agent-reactive mode** — when agent visualization is active, the background canvas shows subtle ripples or trails emanating from positions where agents are active.

### Proxy Management — Full Nginx Capabilities

- [x] **Proxy edit form** — Done: custom response headers, rate limit (req/min), basic auth (htpasswd, SHA1), extended WebSocket timeout/buffering, and gzip+static-cache are all editable alongside the existing domain/SSL/embed/Authentik fields.
- [x] **Proxy presets** — Done: one-click buttons seed the form fields ("Strip iframe blockers", "Add basic auth", "Rate limit 10 req/min", "Force HTTPS redirect", "WebSocket passthrough", "Gzip + cache static assets") — still goes through the existing dry-run preview before saving.
- [ ] **AI proxy recommendations** — when a proxy is created for an App Vault app, VoidTower checks the YAML catalog for known embed requirements and auto-suggests the right preset (Grafana needs CSP relaxed, Portainer needs WebSocket passthrough, etc.). Not started — would need new catalog YAML fields, no existing hooks.
- [x] **Proxy health dashboard** — Done: on-demand reachability check (reqwest GET through the same Docker-host rewrite nginx uses) with a colored status dot, latency, last-checked time, and a manual "Test" button per row. No background polling loop — check is manual-only by design.
- [ ] **Let's Encrypt integration** — SSL certificate request and auto-renewal via Certbot/acme.sh as a toggle in the proxy edit form; renewal status shown in the proxy list.
- [ ] **Wildcard subdomain routing** — configure `*.home.domain.tld` to auto-route to apps by name from a single wildcard proxy rule.

### Search Expansion

Confirmed still accurate: `frontend/src/components/ui/CommandPalette.tsx` is a static `NAV_COMMANDS` list (12 fixed "Go to X" entries) — Ctrl+K today is page navigation only, it does not search any resource content yet.

- [ ] **Full-text search** — Ctrl+K searches across: containers (name/image), services (unit name), timeline events (description), secrets (name only, never value), automation jobs, file names in Files, tags, and notes. Results grouped by type with icons.
- [ ] **Search filters** — filter chips in command bar results: resource type, date range, status, tag. Fully keyboard-navigable.
- [ ] **Saved search shortcuts** — save a query as a named shortcut; invoke with `!name` in command bar.
- [ ] **Inline Odysseus search** — `/query` prefix in command bar sends query to Odysseus and shows the AI response as a result card inline before opening the Odysseus panel.

### AI Creative Studio (Odysseus Voidlink Deep Integration)

Full integration of the Odysseus voidlink automation and generation capabilities into the VoidTower UI. VoidTower becomes a local hub for AI-driven content creation, not just infrastructure management.

**This section was almost entirely marked `[ ]` not-started — that's stale.** `backend/src/api/studio.rs` (585 lines, routes registered at `/api/studio/*` in `api/mod.rs`) and `frontend/src/pages/Studio.tsx` + `frontend/src/aios/panels/studio.tsx` already ship a working AI Studio (commits `b8117d4`, `e7764b7`):

- [x] **AI Studio page** — `Studio.tsx` (Tower Mode) + `aios/panels/studio.tsx` (Void Mode native panel). `studio::status` reports per-service online/offline + version, and GPU VRAM/utilization via `nvidia-smi` shellout. Not done: queue depth display (single-request-at-a-time today, no queue concept yet).
- [x] **Image generation panel** — `studio::image_generate` + `serve_image`; saved under `data/studio/images/`, surfaced in the gallery. Not done: prompt history, workflow picker (txt2img/img2img/inpaint/upscale) — today it's a single generate call, no ComfyUI workflow graph selection.
- [ ] **Video generation panel** — not started, no video diffusion backend wired.
- [x] **TTS panel** — `studio::tts_generate` + `serve_audio`. Not done: voice picker UI, voice cloning workflow.
- [x] **STT panel** — `studio::stt_transcribe`. Not done: SRT output format, "send to Odysseus for summarization" button.
- [ ] **Content pipeline editor** — not started; the existing Automation backend (cron+shell) has no AI node types yet.
- [ ] **3D generation panel** — not started.
- [x] **MCP tool panel** — Done (`e7764b7`): `studio::mcp_tools` / `studio::mcp_invoke` expose the real built-in MCP server's tools (see `backend/src/api/mcp.rs`) for session-authenticated inspect/invoke from the Studio UI, schema-driven form. This satisfies the original ask directly.
- [ ] **Local agent terminal panel** — not started; existing Terminal infra (PTY + xterm.js) could host this but no "Add container logs / Add file" context-button wiring exists yet.
- [ ] **Inbuilt media viewers** — `gallery_list`/`gallery_delete` exist for Studio's own outputs (image/audio), but there's no generic image-gallery/video-player/PDF/3D-model/notebook viewer panel type usable from Files for arbitrary content yet.
- [ ] **Automation library** — not started.

Concrete next steps given what's already there: add a request queue + "queued/running/done" state to `StudioStatus` (it's currently fire-and-synchronous-wait per request), wire ComfyUI workflow selection into `image_generate`, and extend the gallery's media-viewer pattern (already proven for images/audio) into a general-purpose Files viewer for PDFs/notebooks.

---

### Void Mode — /ask Chat Popup

**Substantially complete as of 2026-06-29.** `AiosAskPopup.tsx` is an animated, streaming, dismissible popup wired to focused-panel context. `ai_ask.rs` now routes through the multi-provider AI orchestrator (`backend/src/ai/`) — Odysseus, OpenAI, Anthropic, and local LLM are all supported; the popup exposes a provider selector dropdown.

**Backend**

- [x] **Multi-provider orchestrator** — `backend/src/ai/orchestrator.rs`; providers loaded from `ai_providers` DB table; priority-based routing; per-request `provider_id` override via `POST /api/ai/ask { provider_id }`.
- [x] **Auth gate** — `ai_ask::ask` requires a valid `vt_session` cookie.
- [x] **Graceful degradation** — returns descriptive 400 when no providers configured and legacy `odysseus.allowed_url` is also unset.
- [ ] **`/api/odysseus/*` wildcard reverse proxy** — Not built. `ai_ask::ask` sends to `/api/chat/completions` (or provider equivalent) only; arbitrary Odysseus routes/methods (tool calls, memory, RAG) are not proxied verbatim.

**Frontend**

- [x] **Animated chat popup with provider selector** — `AiosAskPopup.tsx`; lists enabled providers from `GET /api/ai/providers`; "Auto" uses priority order.
- [x] **Context injection** — focused panel title sent as `context` field.
- [ ] **Inline markdown / code block rendering** — plain text only today.
- [ ] **Persistent thread across popup opens** — state resets on each open.
- [ ] **Slash commands (`/ask`, `/clear`, `/copy`) in command bar** — not implemented.

**Frontend: Chat Popup**

- [x] **Animated chat popup** — Done: `AiosAskPopup.tsx` — floating panel, streaming responses, Escape/click-outside dismiss.
- [ ] **Full Odysseus client (transparent-proxy parity)** — Not done — see wildcard-proxy gap above; today's popup only gets chat completions, not tool calls/memory/RAG/MCP passthrough.
- [ ] **Conversational thread** — not verified whether history persists across popup opens within a session; needs a check against `AiosAskPopup.tsx` state management.
- [x] **Context injection (partial)** — Done: focused panel title is sent as `context` to `/api/ai/ask`. Not done: paperclip-pin-live-data as additional context.
- [ ] **Inline rendering (markdown, code blocks, collapsible tool results)** — not verified; needs a check against `AiosAskPopup.tsx`'s render logic.
- [ ] **"Open in Odysseus panel" button** — not verified, likely not implemented.
- [ ] **Slash commands in command bar (`/ask`, `/clear`, `/copy`)** — not verified.

---

### Odysseus Self-Knowledge — Codebase Context at Launch

When Odysseus loads in VoidTower, inject a structured knowledge bundle so the AI knows exactly how VoidTower and Odysseus-Voidlink are built. Ask "how do I add a new background animation?" and get a working template, not a guess.

**This section is also more done than the all-`[ ]` framing suggests.** `backend/src/api/ai_context.rs` (389 lines, route at `GET /api/ai/context`) and the built-in MCP server (`backend/src/api/mcp.rs`, tools `list_routes`/`read_file`/`search_code`/`get_template`) already implement most of the "boot context bundle" idea — just not as a `?vtx=` URL-param injection into an iframe, and not as on-disk editable files.

**What gets injected (the Boot Context Bundle):**

- [x] **Architecture overview** — `ai_context::get_context` returns a static `"architecture"` block (component tree, key paths, handler-pattern snippet) baked into the Rust source, not loaded from a file.
- [x] **Extension templates** — Done, but narrower than the six listed here: `get_template()` actually serves `new_api_endpoint`, `new_tower_page`, `new_native_panel`, `new_background`, `new_catalog_entry`, `new_mcp_tool` (the MCP-tool-deployment-via-Docker and Odysseus-manifest-registration templates described below don't exist as separate templates — `new_mcp_tool` is the closest match).
- [x] **API surface reference** — Done via a different mechanism than planned: not OpenAPI-spec-generated, but `tool_list_routes` greps `.route(` lines straight out of `backend/src/api/mod.rs` at request time, so it's always current and needs no spec generation step.
- [ ] **Current instance state** — not in `get_context`'s payload today (no running-services-count/container-count/alert-count snapshot); this part of the original ask is still open.
- [x] **MCP tools in scope** — the MCP tool list itself (`mcp::tools_json`) already serves this purpose; Odysseus (or anything speaking MCP) can call `tools/list` directly rather than needing it baked into the context bundle.

**Implementation:**

- [x] **`/api/ai/context` endpoint** — Done, registered in `api/mod.rs:188`. Gap vs. the original ask: it returns architecture + templates list only, not the live instance snapshot described above.
- [ ] **Context injection via URL param** — not started; no `vtx=` param anywhere in the tree. The Odysseus iframe is opened without any context pre-seeding today.
- [ ] **Knowledge file storage** — not started; everything in `ai_context.rs` is hardcoded Rust string literals, not files under `data/ai-context/`. This means the architecture doc can't be hand-edited from the Files panel as originally envisioned — worth deciding whether that's even still desirable now that `tool_search_code`/`tool_read_file` let an agent just read the real source directly instead of a maintained summary.
- [ ] **Auto-regenerate on update** — not applicable in the current design (nothing is generated-and-stored, it's computed per-request), would need rethinking if knowledge file storage above is built.
- [ ] **`/api/ai/context/templates`** — not a separate route; `get_template(name)` is currently invoked through `/api/mcp` (tool call `get_template`) or the Studio MCP panel, not as its own discoverable REST endpoint. Adding `GET /api/ai/context/templates` and `GET /api/ai/context/templates/:name` would be a small, natural follow-up since `templates_list()` already exists internally.

---

### Proxmox Full Management Suite

Full Proxmox Virtual Environment management built into VoidTower — on par with PVEDiscord/PegaProx for depth, but native to the AIOS panel system. No separate web UI needed.

**This section was almost entirely marked `[ ]` — significantly stale.** `backend/src/api/proxmox.rs` (976 lines) and `frontend/src/pages/ProxmoxPage.tsx` (1093 lines) already ship multi-host CRUD, a combined VM/LXC table with lifecycle actions, snapshots, noVNC console, storage pool listing, task history, and PBS-style backup aggregation. Verified directly against source (not the doc's prior claims):

**Proxmox Tower Mode pages (sub-navigation under VMs):**

- [x] **Nodes overview** — Done (2026-06-24): `NodeCards` shows per-node CPU/RAM/root-disk usage bars, uptime, kernel version (`kversion`, already returned by `/status`, now rendered), and a subscription badge (`subscription_status` from `/nodes/{node}/subscription`, merged server-side in `list_nodes`). Still missing: click-to-filter-other-views.
- [~] **QEMU VMs** — Partial: combined VMs&LXCs table (`VmsTable`) with vmid/name/status/CPU%/RAM/disk/node/tags, search + status/type filters, sortable columns, bulk row-select (start/stop/tag) (2026-06-24). Actions: start/stop(graceful shutdown)/reboot/reset/suspend/resume(QEMU only)/console/snapshot (2026-06-24). Missing: migrate action, network I/O column. Polls every 5s (2026-06-24, was 15s).
- [ ] **LXC Containers** — Not a separate page; LXC rows live in the same combined table as QEMU (filterable by type), not split out with LXC-specific fields (unprivileged flag, ostype). The standalone local-LXC page (`/lxc` via `pct`, per Phase 4) is a different, non-Proxmox-API feature — don't conflate the two.
- [x] **Console access** — Done: `vm_vncproxy` (backend) + `ConsoleModal` (frontend, CDN noVNC) — works for both QEMU and LXC via the detected `kind`. CPR (`\x1b[6n`) handling not relevant here (that's the Terminal PTY feature); this is pure VNC framebuffer.
- [ ] **VM creation wizard** — Not started for general QEMU VM creation. Related but distinct: `deploy_app_to_lxc` already creates a fresh LXC container + bootstraps Docker + an App Vault compose file onto it (`DeployToProxmoxModal.tsx`) — that's an App-Vault-to-LXC deploy flow, not a general "create any VM" wizard with ISO/cloud-init.
- [x] **Snapshot management** — Done: `vm_snapshot`/`vm_rollback`/`vm_delete_snapshot`/`list_snapshots` backend + `SnapshotRow`/`CreateSnapshotModal` frontend (expandable per-VM row). Dry-run now routes through `ChangePlanModal` for stop/reset/suspend/rollback/delete-snapshot/snapshot-create (2026-06-24) — `vm_start`/`vm_reboot` also gained `dry_run` server-side for completeness, though those two stay direct-execute in the UI (low/expected-frequency actions, no confirmation step added).
- [x] **Storage browser** — Done (2026-06-24): each pool row in `StorageTable` expands into a content browser (`list_storage_content`/`upload_storage_content`/`delete_storage_content` in `proxmox.rs`) — lists ISOs/templates/disk images/backups per pool, uploads ISOs/container templates via multipart, deletes content through the `ChangePlanModal` dry-run gate. New **Disks** tab (`DisksPanel`) adds physical-disk management: `list_node_disks`/`disk_smart`/`wipe_disk`/`init_disk_storage` — per-node disk table with model/size/health, a SMART-data modal, wipe (dry-run gated, high risk), and initialize-as-storage (directory/LVM/LVM-thin/ZFS, dry-run gated). Disk passthrough (`vm_disk_passthrough`) attaches a raw host block device directly to a QEMU VM's `/config` (`scsiN=/dev/sdX`-style), dry-run gated, QEMU-only. Not done: Files-page virtual-root browsing of Proxmox storage content (scoped out — would need a non-local-path abstraction in `files.rs`/`Files.tsx` that doesn't exist today); restic-backup-target integration for Proxmox storage (opt-in, not yet wired).
- [~] **Backup jobs** — Partial: `list_backup_jobs` aggregates cluster-level scheduled jobs + a cross-node archive listing (`BackupsPanel`) — but there's no restore-to-new-VM/LXC wizard, and it's a generic backup-storage scan, not a dedicated PBS datastore/namespace browser (see PBS section below).
- [~] **Cluster / node tasks** — Partial: `list_tasks` + `TasksTable` shows recent task history with status/duration across nodes. Missing: live progress bars for in-flight ops, subscription/quorum health.
- [ ] **Network & firewall** — Not started — no SDN/bridge/firewall-rule routes exist in `proxmox.rs`.

**Void Mode native panel:**

- [x] **NativeProxmoxPanel** — Done (2026-06-24): `aios/panels/proxmox.tsx` now ships four tabs — VMs / Storage / Tasks / Snapshots (not "VMs / LXC / Snapshots" as originally written; LXC isn't split out, same combined-table reasoning as the Tower Mode page above). Console button added per running VM row (reuses the exported `ConsoleModal` from `ProxmoxPage.tsx`); Snapshots tab lists per-VM snapshots with rollback/delete routed through `ChangePlanModal`.

**Multi-host:**

- [x] **Multiple Proxmox hosts** — Done, and was wrongly marked not-started: host CRUD (`list_hosts`/`create_host`/`delete_host`) already supports any number of hosts; the sidebar in `ProxmoxPage.tsx` lists all configured hosts with add/delete, and the native panel has a host-switcher `<select>` when more than one exists.

**Proxmox Backup Server (PBS) integration:**

- [ ] **PBS datastore browser** — Not started as a true PBS integration (datastores/namespaces/GC status via the PBS API). What exists (`list_backup_jobs`) talks to the PVE API's own storage/backup-content listing, which works whether or not PBS specifically is the backend — it does not call a PBS instance directly.

**Safety:**

- [x] Done (2026-06-24): `vm_stop`/`vm_reset`/`vm_suspend`/`vm_rollback`/`vm_delete_snapshot`/`vm_snapshot` all route their `dry_run` response through the shared `change_plan()` helper (`backend/src/api/proxmox.rs`) into `ChangePlanModal` on both the Tower Mode page and the native panel. `vm_start`/`vm_reboot` gained `dry_run` support server-side too, but the UI still calls them directly (no confirmation step — these are low-risk/frequent actions).
- RBAC today is coarser than described: every Proxmox route uses a single `require_admin` (owner/admin only) — there's no Operator-can-start-stop / Admin-can-snapshot / Owner-can-configure-clusters tiering yet; that tiering is still aspirational.

---

## Self-Hosted Email Service

Not started — nothing in `app-vault/apps/`, `backend/src/api/`, or the proxy system today is mail-specific. New section, added 2026-06-24 at user request. Mail is a different risk class from everything else in App Vault: a misconfigured deploy doesn't just break a container, it gets the host's IP blacklisted or silently drops outbound mail with no error visible to the user. The plan below is written to surface that risk up front rather than ship a one-click "Deploy Email Server" button that quietly fails in production.

**Reality check before any of this is built:**

- [ ] **Residential/ISP feasibility check** — most residential ISPs block outbound port 25 and won't grant a reverse DNS (PTR) record for a dynamic IP, both of which are required for other mail servers to accept your mail. Before this ships, add a guided checklist (or automated check: open an SMTP probe to a known relay, check for PTR via DNS lookup) that tells the user up front whether direct self-hosted delivery will even work on their connection, and steers them to the smarthost-relay path below if not.
- [ ] **Smarthost relay fallback** — for the common case where outbound 25 is blocked, support routing outbound mail through a relay (a cheap VPS, or a transactional provider like Mailgun/SES/Postmark used purely as a relay, not as the inbox). This is the difference between "self-hosted mail that actually delivers" and "self-hosted mail that lands in spam or nowhere."

**App Vault deployment:**

- [ ] **Mail server catalog entry** — new `app-vault/apps/` YAML following the existing pattern (see `nextcloud.yml`/`vaultwarden.yml` for the compose+env-var conventions). Candidates to evaluate: `docker-mailserver` (Postfix+Dovecot+Rspamd+ClamAV, most widely deployed, config-via-env-vars fits the YAML-catalog model well), Mailcow (heavier, full admin UI of its own — would partially duplicate VoidTower's own management page below), or Stalwart Mail Server (single Rust binary, JMAP-native, modern and a closer architectural fit to VoidTower's own stack, but younger/less battle-tested). Lean toward `docker-mailserver` first for the install-base and documentation depth; Stalwart as a phase-2 "modern option" once the integration patterns below are proven.
- [ ] **Webmail UI** — Roundcube or SOGo as a second compose service, proxied through VoidTower's existing nginx proxy system exactly like any other App Vault app (`write_nginx_conf`, Docker-host upstream rewrite already required by `CLAUDE.md`'s nginx rule applies here too).

**VoidTower-native mail management page:**

- [ ] **Mailboxes & aliases** — Settings-style page to create/delete mailboxes, set quotas, manage aliases/forwards, and reset mailbox passwords — calling the mail server's own admin API/CLI (`docker-mailserver` ships a `setup.sh` exec-style admin interface; Stalwart has a JMAP/REST admin API) rather than reimplementing mail storage logic. Gated by RBAC: Operator can't touch mail config, Admin can manage mailboxes, Owner can add/remove domains.
- [ ] **DNS record helper** — generates the exact SPF, DKIM, DMARC, and MX records the user needs to paste into their DNS provider (or, if Technitium DNS — already on the App Vault wishlist above — is deployed, offers to write them automatically via its API). Includes a "verify" button that does a live DNS lookup and shows which records are correctly propagated vs. missing.
- [ ] **Deliverability dashboard** — RBL/blacklist check (query common DNSBLs for the sending IP), bounce rate, queue depth, and a "test send" button that round-trips a message through an external mailbox-checking service so the user gets a real spam-score signal instead of just "the command exited 0."
- [ ] **Backup integration** — wire mailbox storage volumes into the existing restic Backups module (same pattern as every other stateful App Vault deployment) rather than inventing a separate mail-backup mechanism.
- [ ] **Resource tags & alerts** — tag mail domains/mailboxes like any other resource (`"mail_domain"` resource type alongside the existing `proxy`/`container`/`service`/`backup`/`app`/`proxmox_vm` set); alert rules for queue backlog, repeated auth failures (brute-force probing is constant against any mail server with port 25/587 open), and blacklist hits, using the existing Alerts module pattern (metric threshold + TCP/HTTP-style checks).

**Safety:**

- Mailbox/domain create, delete, and DKIM-key rotation go through the existing `ChangePlanModal` dry-run pattern — domain delete especially, since it can silently black-hole inbound mail for that domain if done wrong.
- Default-deny: don't open port 25/587/993 on the firewall automatically the way App Vault embed ports are auto-opened today — mail ports are a much bigger attack surface than a typical app's web UI, so this should be an explicit opt-in step with the reality-check warnings above shown first.

---

## Self-Hosted Home Hub — Cloud-Service Replacement Wishlist

Nice-to-haves, not commitments — lower priority than Phase 5, Proxmox, and Email above. The vision: a household runs its entire digital life through VoidTower instead of scattering it across Google/Apple/Meta/cloud SaaS, with Odysseus/Studio as the AI glue rather than a separate cloud assistant. Every item below should slot into an existing VoidTower pattern (App Vault YAML catalog, Tags, Alerts, Dashboard widgets, RBAC) rather than inventing a parallel subsystem — that's called out per item. None of this is started; nothing below should be read as "in progress."

**Every domain below gets full AI integration through the existing Voidwatch/Odysseus contract, not a bespoke one.** Per `docs/integrations/odysseus.md`, Voidwatch already consumes VoidTower purely through things that already exist: a scoped API token with per-domain read/action permission scopes, MCP tools exposed by the built-in server (`backend/src/api/mcp.rs`), and signed webhook events that let Voidwatch spin up an Odysseus investigation task whenever something fires. So for each new domain (voice assistant, calendar, location, finance, bookmarks, documents, notifications, energy, parental controls), the AI-integration checklist is the same four things, reused rather than reinvented:
1. **Scopes** — new permission scopes (e.g. `calendar:read`, `finance:read`, `location:read`) added to the existing API-token scope list so a Voidwatch token can be granted exactly the access a household wants an agent to have.
2. **MCP tools** — new tools registered alongside the existing `list_nodes`/`list_containers`/`list_alerts`/etc. set (e.g. `list_calendar_events`, `get_budget_summary`, `get_last_known_location`), so "ask Odysseus" works the same way for a calendar gap as it does for a failed container today.
3. **Webhook events** — domain-specific events (geofence crossed, budget threshold hit, voice intent unresolved) pushed through the same signed-webhook bridge that today fires on alerts/service failures, so Voidwatch can react to home-hub events exactly like infra events.
4. **Policy gating** — any AI-triggered write (create a calendar event, flag a transaction, change a parental-control schedule) goes through `policy::check` the same way webhook-triggered automations already do, so "what can the AI actually do here" stays a per-rule decision instead of an all-or-nothing token grant.

### Local Voice Assistant (replaces Alexa / Google Home)

The biggest synergy item here — VoidTower already has every primitive a fully local voice pipeline needs, just not wired together for this purpose. This one is Voidwatch-native by design, not just Voidwatch-integrated: the assistant's "brain" should be Odysseus itself.

- [ ] **Wake-word → STT → LLM → TTS loop** — `studio::stt_transcribe` and `studio::tts_generate` already exist; pairing them with a wake-word listener (openWakeWord) and routing the transcript to Odysseus closes the loop without a new AI backend.
- [ ] **Satellite hardware support** — ESP32-based voice satellites (Home Assistant Voice PE, or any ESPHome `assist_satellite`) stream audio in, VoidTower's pipeline above processes it, response streams back as TTS audio.
- [ ] **Agentic intent resolution via Odysseus/MCP, not a fixed grammar** — rather than a small hand-coded grammar, the transcript goes to Odysseus as a normal chat turn with MCP tools available; "turn off the living room lights" becomes a tool call against Home Assistant's service API (exposed as an MCP tool, same pattern as every other home-hub domain above), and "what's on the calendar today" becomes a `list_calendar_events` call — so the voice assistant gets every other Home Hub item's AI integration for free as those tools come online, instead of needing its own NLU stack.

### Calendar & Contacts (CalDAV/CardDAV)

- [ ] **App Vault catalog entry** — Radicale (lightweight, single binary) or Baikal (web admin UI) following the existing compose+env-var YAML pattern.
- [ ] **Family shared calendars** — native page surfacing events from the deployed CalDAV server, reusing the Dashboard widget system for an "upcoming events" widget rather than building a separate calendar app.
- [ ] **Maintenance-window tie-in** — once Maintenance windows (Feature Backlog → Operations) exist, they could optionally publish as calendar events on the same CalDAV server, so "what's scheduled" is visible from any calendar client, not just VoidTower.
- [ ] **Voidwatch/Odysseus tooling** — `calendar:read`/`calendar:write` scopes, `list_calendar_events`/`create_calendar_event` MCP tools, and a webhook event on "event starting soon" so Odysseus can proactively remind a household member — gated through `policy::check` since creating events is a write action.

### Family Location Sharing (replaces Find My / Life360)

- [ ] **App Vault catalog entry** — OwnTracks (MQTT-based, simplest) or Dawarich (richer history/maps UI).
- [ ] **Geofence alerts** — reuse the existing Alerts module pattern (metric threshold / TCP-HTTP checks already exist as a model) for "arrived home" / "left geofence" events instead of a new notification path.
- [ ] **Per-person resource tags** — tag each tracked device/person like any other resource, consistent with the existing tag system rather than a separate household-member concept.
- [ ] **Voidwatch/Odysseus tooling** — `location:read` scope (read-only by default — this is the most privacy-sensitive domain in the whole hub), a `get_last_known_location`/`list_geofence_events` MCP tool, and geofence-crossing events routed through the signed webhook bridge so "is everyone home yet" is answerable by asking Odysseus instead of opening a map.

### Personal Finance / Budgeting

- [ ] **App Vault catalog entry** — Firefly III or Actual Budget, replacing Mint/cloud budgeting apps. Both are standard Docker deploys that fit the existing YAML catalog model with no special-casing needed.
- [ ] **Voidwatch/Odysseus tooling** — `finance:read` scope, a `get_budget_summary`/`list_recent_transactions` MCP tool, and a budget-threshold-exceeded event through the webhook bridge (reusing the Alerts module's threshold pattern) so Odysseus can flag overspending the same way it flags a failing service. Any write capability (categorizing a transaction, adjusting a budget) is opt-in and policy-gated separately from read access.

### Bookmarks & Read-it-later

- [ ] **App Vault catalog entry** — Linkwarden or Karakeep, replacing Pocket/cloud bookmarking. Natural fit for the Search Expansion backlog item once built (full-text search across saved links alongside containers/services/files).
- [ ] **Voidwatch/Odysseus tooling** — `bookmarks:read`/`bookmarks:write` scopes and `list_bookmarks`/`save_bookmark` MCP tools, so "save this and summarize it" can be a single Odysseus turn instead of a manual copy-paste into the app.

### Document Signing & Forms

- [ ] **App Vault catalog entry** — Docuseal (e-signatures) and/or Formbricks (forms/surveys), replacing DocuSign/Google Forms for household paperwork.
- [ ] **Voidwatch/Odysseus tooling** — `documents:read` scope and a `list_pending_signatures` MCP tool plus a webhook event on "signature requested"/"signature completed," so Odysseus can nudge whoever hasn't signed yet — no write/sign capability exposed to AI by default, this domain is read/notify-only.

### Push Notification Hub

- [ ] **Ntfy as VoidTower's own alert delivery channel** — Ntfy is already on the App Vault wishlist (see Planned Apps below) purely as a deployable app; the nice-to-have here is wiring it as a *delivery channel* for VoidTower's existing Alerts module (alongside whatever channels Alerts already supports), so self-hosted push notifications reach phones without depending on a third-party push provider.
- [ ] **Voidwatch/Odysseus tooling** — this one's the delivery mechanism for every other domain's webhook events above, not a new MCP tool itself: once Ntfy is wired as an Alerts channel, Odysseus-triggered notifications (geofence, budget, signature, calendar reminders) ride the same path as infra alerts do today, with no separate push integration needed per domain.

### Home Energy / IoT on the Dashboard

- [ ] **Home Assistant data as native Dashboard widgets** — Home Assistant is already a deployable App Vault app; pulling its entity states (energy usage, sensor data) into VoidTower's existing customizable-widget Dashboard would put household telemetry next to CPU/RAM/disk widgets instead of requiring a separate HA dashboard tab.
- [ ] **Voidwatch/Odysseus tooling** — `iot:read` scope and a `get_entity_state`/`list_iot_entities` MCP tool wrapping Home Assistant's own API, so "is the garage door open" or "how much power is the dryer using" is answerable by Odysseus directly, and unusual-usage alerts (a sensor pattern outside its normal range) route through the existing Alerts module the same way a metric-threshold alert does today.

### Parental Controls / Family Device Management

- [ ] **Per-device-group Pi-hole/AdGuard policies as a native page** — both are already deployable App Vault apps with group-based client policies; a thin VoidTower page wrapping their group/client API (same "call the app's own admin API rather than reimplement it" approach as the Email section's mailbox management) would surface "kids' devices offline after 9pm"-style controls without leaving VoidTower.
- [ ] **Voidwatch/Odysseus tooling** — `parental:read` scope (and a separate, explicitly opt-in `parental:write` scope for schedule changes) plus a `list_device_group_status` MCP tool — any AI-triggered schedule change is policy-gated and audit-logged like every other write action in this list, since this is one of the higher-trust domains in the hub.

---

## Feature Backlog (roughly prioritized)

### Infrastructure intelligence

- [ ] Config drift detection — detect when systemd units, Docker Compose files, firewall rules, or proxy configs change outside VoidTower; show expected vs actual diff with reconcile/accept options
- [ ] Inventory and asset database — lightweight CMDB: hardware, CPU, RAM, disk, GPU, OS/kernel versions, installed packages, owners, notes, warranty metadata per node
- [ ] Declarative state mode (GitOps) — export current state as YAML, import desired state, dry-run apply, optional Git sync
- [ ] App Vault image-tag drift — compare each deployed app's running image tag against the catalog YAML's `version_hint`; flag out-of-date deployments on the App Vault page instead of only surfacing this through the separate Updates module
- [ ] Resource dependency graph — VoidTower already tracks proxy→upstream relationships (`proxy_configs.embed_port`/upstream URL) and resource tags across containers/services/VMs/backups; a graph view answering "what breaks if I stop this container" (which proxies point at it, which automations reference it) would reuse that existing data rather than needing new tracking

### Operations

- [ ] Maintenance windows — schedule windows, suppress alerts during them, allow selected automations only, notify before/after
- [ ] Incident mode — create incident from alert, attach logs/metrics/services, owner assignment, status tracking, postmortem export, Odysseus investigation handoff
- [ ] Notes per resource — Markdown notes pinnable on nodes, containers, VMs, services, alerts, automations, backups, security findings
- [ ] Snapshot-aware operations — create VM/Btrfs/ZFS/Compose rollback points before dangerous changes; UI to compare before/after and roll back
- [ ] Update management — controlled updates for VoidTower, App Vault templates, Docker images, OS packages, and (future) plugins with changelog, risk level, dry-run, rollback
- [ ] Bulk/multi-select actions — start/stop/restart/tag across multiple containers, services, or VMs at once; the Proxmox suite (above) already calls for bulk-select on VM rows, this would generalize the pattern to Containers/Services pages too

### AI / Odysseus depth

- [x] Policy engine for Odysseus actions (see must-have #6 above) — done in P0: `voidwatch::evaluate()` gates MCP `tools/call` and Studio `mcp_invoke` the same way webhook-triggered automations already were, plus default-deny, mode ladder, and the irreversibility denylist on top.
- [x] "Send to Odysseus" buttons — Done, in copy-to-clipboard form: `frontend/src/components/ui/SendToOdysseus.tsx`, wired into Alerts/Services/Containers. Not done: full context packaging with secret redaction (Odysseus has no `?prompt=` param to receive it directly) and extending the button to VMs/backup failures/security findings/log selections.
- [ ] AI approval queue UI — pending high-risk AI-requested actions; approve once / deny / approve with time limit / create policy from repeated safe action
- [ ] Full event stream subscriptions — currently SSE endpoint exists; needs: webhook outbound push, MCP resource/event support for agents
- [ ] MCP tool-call audit trail — every `mcp::invoke_tool` call (direct MCP, or via Studio's `mcp_invoke`) should write an `audit::log` entry the same way `proxmox.vm.stop`/`proxmox.vm.snapshot` etc. already do, so "what did the AI actually run" is answerable from the Timeline without cross-referencing Odysseus's own logs.

### Developer experience

- [ ] Public API SDKs — generate TypeScript, Python, Go SDKs from the existing OpenAPI spec (note: no OpenAPI spec exists yet — this depends on the item below shipping first)
- [ ] OpenAPI documentation UI — confirmed not started, no `openapi` references anywhere in the tree; expose `/api/openapi.json` with a browsable Swagger/Redoc UI
- [ ] Demo / simulation mode — confirmed not started, no `--demo` flag in `main.rs`'s arg parser; `voidtower --demo` creates fake nodes/metrics/alerts/containers/VMs/backups/automation runs; does not touch real host
- [ ] Expand native CLI beyond `user`/`backup` — `voidtower` already has `user list/create/reset-password/set-role/delete` and `backup list/create/run/check/restore-test/delete` (see CLAUDE.md "CLI management commands"); the same open-DB-and-exit pattern extends naturally to `voidtower proxy list/create/delete`, `voidtower app deploy/list`, and `voidtower policy list` for headless/scripted administration without booting the web server

### App Vault expansion

- [x] Custom app deployment form — Done: `CustomDeployTab` in `frontend/src/pages/AppVault.tsx:196` (wired into the page at line 1398) — image, name, port map, volume map, env vars → generates and saves a compose file, no YAML knowledge needed.
- [ ] AI integration badges — per-app badge tier: AI Native / AI Aware / AI Ready / none; defined in YAML catalog; shown as colored chip on app card
- [ ] Per-app resource limits — CPU/memory caps surfaced in the existing compose editor (staged-diff pattern already built for Containers) instead of requiring the user to hand-edit the compose YAML to add `deploy.resources.limits`
- [ ] Servarr stack pairing hints — now that the full Servarr suite (Sonarr/Radarr/Lidarr/Readarr/Prowlarr/Bazarr/Seerr/qBittorrent/Gluetun/Recyclarr/FlareSolverr) is in the catalog, a "deploy as a pre-wired group" option (shared download client + indexer config) would save the multi-app manual wiring these apps normally require

### VM / Android hosting

- [ ] GPU passthrough UI — assign a physical GPU to a KVM VM with a toggle (UI only; libvirt XML generation)
- [ ] ISO library browser — upload or link ISOs for new VM creation
- [ ] VM snapshot management UI — Proxmox and libvirt snapshot create/restore/delete
- [ ] Waydroid instance manager — manage Waydroid Android containers, expose via browser stream (scrcpy → WebRTC). Confirmed not started — no `waydroid` references anywhere in the tree.
- [ ] Android-x86 in QEMU — isolated Android VMs for testing
- [~] Redroid support — **partially done, this line was stale.** `app-vault/apps/redroid.yml` already deploys a single Redroid instance + scrcpy-web browser viewer via the standard App Vault one-click flow. Still missing: multi-instance management (spin up/tear down several Redroid containers per host), and ADB control from VoidTower itself rather than just the scrcpy video stream.

---

## App Vault — Planned Apps

**Corrected count: 54 apps present** (`ls app-vault/apps/*.yml | wc -l`). The original "already in vault" list also self-contradicted the table below — it listed `jitsi` and `matrix-synapse` as already present while the planned-apps table *also* listed "Matrix / Synapse" and "Jitsi" as not-yet-added. Both are removed from the planned table below since they're confirmed present.

Apps mentioned in `future_plan.md` section 21 that are **still not** in `app-vault/apps/`:

| App | Category | Notes |
|---|---|---|
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
| Stable Diffusion WebUI | AI | `comfyui.yml` already covers Stable Diffusion via ComfyUI — a dedicated AUTOMATIC1111-style WebUI is still missing |
| Whisper (standalone) | AI | `studio.rs`'s STT panel covers transcription in-app; a standalone deployable Whisper API container for other apps to call is still missing |

Apps already in `app-vault/apps/` (54 present): adguardhome, anythingllm, authentik, bazarr, changedetection, code-server, comfyui, dozzle, eurooffice, flaresolverr, freshrss, gitea, gluetun, grafana, homeassistant, immich, jellyfin, jitsi, kavita, lidarr, librechat, llama-cpp, matrix-synapse, mealie, minio, n8n, navidrome, nextcloud, nginx-proxy, odysseus, ollama, opencloud, open-webui, outline, paperless, pihole, portainer, prowlarr, qbittorrent, radarr, readarr, recyclarr, redroid, searxng, seerr, sonarr, stirling-pdf, syncthing, tailscale, uptime-kuma, vaultwarden, vikunja, wireguard-easy, youkidex.

A full Servarr media-management stack (`bazarr`, `flaresolverr`, `gluetun`, `seerr`, `lidarr`, `prowlarr`, `qbittorrent`, `radarr`, `readarr`, `recyclarr`, `sonarr`) has landed since this doc was last written and isn't mentioned anywhere above it — worth its own "Media Automation" row in the Current State table rather than being buried in a flat app list.

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
| MCP server (built-in, tool-serving) | **Implemented** — `backend/src/api/mcp.rs` (commit `3a23ed3`), real JSON-RPC SSE+message server at `/api/mcp` + `/api/mcp/message` with 13 tools (`list_nodes`, `get_node_metrics`, `list_containers`, `list_services`, `list_alerts`, `get_container_logs`, `list_routes`, `read_file`, `search_code`, `get_template`, `list_secrets`, `get_policy_rules`, `get_audit_log`). Studio's MCP tool panel (`studio::mcp_tools`/`mcp_invoke`) reuses the same tool set. |
| App-specific MCP servers (standalone) | **Implemented** (`11bade2`) — `odysseus-mcp-servers/voidtower_server.py` expanded to 38 tools (proxy, firewall, VM lifecycle, App Vault deploy/undeploy, service logs, tags, status checks). 39 new per-app standalone Python MCP servers ship alongside it, one per App Vault catalog app, ~413 tools total. Each registers independently in Odysseus or any MCP client; see `docs/integrations/mcp-server.md`. |
| "Send to Odysseus" buttons in UI | Implemented (copy-to-clipboard variant) — `SendToOdysseus.tsx`, wired into Alerts/Services/Containers; no full context-packaging-with-redaction yet |
| AI approval queue / pending action UI | Not implemented |
| Event stream webhook outbound push | Not implemented — SSE only |
| Per-Odysseus-action policy enforcement | Done (P0) — `voidwatch::evaluate()` is now the single choke point for MCP `tools/call`, Studio `mcp_invoke`, and webhook/automation actions; default-deny + mode ladder + irreversibility denylist all apply. Direct MCP/Studio tool calls no longer bypass policy. |
| Odysseus integration events linked to timeline | Partial — audit log exists; no explicit Odysseus tagging |
| Voidwatch toolpack definitions | Present in `voidwatch/toolpacks/` (20 packs) — see README |

---

## Known Issues / Tech Debt

- **Pi-hole pinned to v5** — ~~`2024.07.0`, v6 changed config format~~ Done — `app-vault/apps/pihole.yml` now deploys `pihole/pihole:latest` (v6) with inline comments documenting the v5→v6 env-var migration (`WEBPASSWORD`→`FTLCONF_webserver_api_password`, etc.)
- **Odysseus/Ollama dual-deploy port conflict** — ~~still open~~ Done (`ddde48e`) — `app-vault/apps/ollama.yml`/`odysseus.yml` set `system_conflict_check: ollama`/`odysseus`; `apps::deploy` (`backend/src/api/apps.rs:845-861`) checks for the matching `/var/lib/voidtower/.{key}-system-installed` marker before deploying and returns a `BadRequest` with the port conflict and `systemctl stop/disable` instructions if found.
- **YoukiDex APK sideload not automated** — still open. `youkidex.yml`'s `setup_notes` still requires manual `adb connect`/`adb install YoukiDex-v2.7.apk`/permission-grant steps after deploy.
- **App Vault embed steps not complete** — ~~partially wired but not automatic for all apps~~ Done — `apps.rs::open_ui` generically allocates an embed port, writes the nginx conf with X-Frame-Options stripping, reloads nginx, and opens the firewall port for any app on every call, not app-specific.
- **Odysseus `voidlink-latest` Docker image CI workflow** — split into two, since the original entry conflated them: (1) **Done** for VoidTower's own image — `.github/workflows/docker.yml` builds and pushes `ghcr.io/niwlekakan/voidtower:aio-latest` automatically on push to `main` (formerly `voidtower-aio`, promoted to `main` 2026-07-07). (2) **Still open** for Odysseus's own `voidlink-latest` image — that's a separate repo (`niwlekakan/odysseus`, branch `odysseus-voidlink`); this repo's CI can't build or verify it.
- **CLI commands not on PATH after bare-metal install** — ~~`voidtower user list` etc. failed after `install.sh` because the binary at `/opt/voidtower/voidtower` was never linked into PATH~~ Done — `install_path_symlink()` in `install.sh` creates `/usr/local/bin/voidtower → /opt/voidtower/voidtower` on install, `--update`, and `--repair`; `--uninstall` removes it. Docker installs already had the binary at `/usr/local/bin/voidtower` inside the image.
- **TrueNAS AIO end-to-end test pending** — code-level gap is fixed: `deploy/truenas/custom-app.yml` already parameterizes `TRUENAS_POOL` (default `tank`) everywhere a host path is used, and `.env.example` documents setting it to `main`. What's left is a real deploy-and-verify pass, not a code fix.
- **MCP server** — ~~stub~~ Done (`3a23ed3`) — full SSE+message MCP server with 13 tools
- **LXC management** — ~~missing~~ Done (`418963f`) — `/lxc` page + `pct` backend
- **Agent/multi-node mode missing** — still open. `backend/src/cluster/mod.rs` is only an `#[allow(dead_code)]` `is_agent_mode()` checking `--agent`, plus a comment block describing the unbuilt plan — no `/agent/metrics`/`/agent/actions` routes or join-token system exist.
- **TOTP** — ~~`totp.rs` exists but no UI~~ Done (`16b3a59`) — Security page + login step

---

## Not Planned

- **More themes** — Odysseus ROADMAP: "I prob shouldnt add more themes"; same applies to VoidTower. Already has 23 built-in themes (`frontend/src/theme/themes.ts`) — this is "stop adding," not "none exist yet"
- **iPhone / iOS VM** — no legal option exists; Corellium is paid/enterprise only; not in scope
- **macOS VM** — OSX-KVM is legal grey area; not in scope for mainline
- **Cloud dependency / telemetry / license server** — never; out of scope by principle
- **Kubernetes / etcd / external consensus** — plan.md explicitly excluded for MVP; not planned
