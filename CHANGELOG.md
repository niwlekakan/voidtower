# Changelog

All notable changes to VoidTower are documented in this file.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Versioning is [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.9.0] — 2026-06-08

First public release milestone. All planned must-have features are complete.

### Added — Core platform

- **Installer** (`scripts/install.sh`) — interactive, four install paths: bare-metal/LXC, Docker, TrueNAS Option A (Compose), TrueNAS Option B (Custom App + SSH). Auto-detects and installs Docker Engine on fresh systems. Opens firewall ports automatically (ufw → firewalld → iptables fallback). Shows LAN IP in post-install summary.
- **Proxmox LXC installer** — dedicated `install-lxc.sh` for deploying VoidTower as a Proxmox container.
- **Bootstrap token** — re-logged on every restart until first admin account is created; visible via `--show-token`.
- **Doctor / diagnostics** — `--doctor [--json]` CLI flag; mirrors `/api/diagnostics` checks; exits with code 1 on failures.
- **Disaster recovery** — `--export-config` / `--import-config` CLI flags; emergency admin reset and instance disable endpoints; UI page at `/disaster`.
- **Recovery & Maintenance** docs — README covers admin access recovery, Odysseus password reset (all 4 install paths), full reset, full reinstall, repair.

### Added — Security

- **Policy engine** — DB-backed rules that govern what automated actors (API tokens, automations) may do. Additive restrictions on top of token scopes. Default deny rules seeded on install: `ai-no-touch` resources, `critical` resources, API token `remove` actions. CRUD API + UI at `/policy`.
- **Secrets manager** — AES-256-GCM encrypted secrets table. Reveal, rotate (version counter), per-token secret scope restriction, disaster-recovery export/import.
- **TOTP 2FA** — setup/enable/disable in Security page; enforced at login.
- **Rate limiting** — failed login lockout with per-IP tracking.
- **Security headers** — `X-Frame-Options`, `X-Content-Type-Options`, `Referrer-Policy`, `Content-Security-Policy` on all responses.
- **SSRF guard** — blocks server-side requests to private/loopback ranges.
- **Session management** — active session list with revoke-others in Security page.
- **Audit log** — all state-changing operations recorded with actor, action, source.
- **API tokens** — scoped (`read`, `write:containers`, etc.), bearer auth with automatic session injection, per-token secret restriction.

### Added — Proxmox integration (Phase 3)

- **Multi-host management** — add/remove Proxmox hosts; tokens stored encrypted.
- **Read-only views** — nodes, VMs, LXCs, storage, task log, PBS backup jobs.
- **Lifecycle actions** — start, stop, reboot, shutdown, snapshot, rollback, delete snapshot. Dry-run / change-plan guard on destructive actions.
- **noVNC console** — in-browser VM console via Proxmox vncwebsocket; no local noVNC install needed (CDN).
- **VM alerts** — 90-second poll loop; fires alerts on unexpected state transitions.
- **Resource tags** — tag Proxmox VMs alongside containers, services, apps, proxies, backups.
- **PBS backup tab** — lists backup jobs and recent archives per host.
- **AIOS panel** — Proxmox Tower page in Void Mode.
- **Deploy App Vault → Proxmox LXC** — creates an LXC via Proxmox API (auto-VMID, nesting, DHCP, onboot), returns a copy-pasteable Docker bootstrap script.

### Added — AI integration (Phase 4)

- **Odysseus integration** — scoped API tokens, SSE event stream, webhook handler, manifest endpoint.
- **AI context endpoint** — `/api/ai/context` returns a structured codebase map for Odysseus system prompt injection.
- **MCP server** — Model Context Protocol server at `/api/mcp` with tools: `list_routes`, `read_file`, `search_code`, `get_template`, `list_containers`, `list_services`, `list_secrets`, `get_policy_rules`, `get_audit_log`, `get_alerts`, `get_tags`, `get_timeline`, `get_backups`.
- **Odysseus webhook** — handles `container.*` and `service.*` actions (start/stop/restart) dispatched from automations; policy-checked.
- **AI ask popup** — `/ask` chat overlay wired to local llama.cpp or Ollama.

### Added — App Vault

- **Catalog** — 40+ curated self-hosted apps with Docker Compose definitions, port mappings, AI integration badges.
- **Deploy** — one-click deploy with automatic conflict detection (system-installed services), GPU requirement stripping.
- **Embed proxy** — port-based nginx proxy (8800–8899 range) for in-UI app iframes; no DNS required; firewall opened automatically.
- **Custom deploy** — paste any Docker Hub image; configure ports, volumes, env vars; live YAML preview.
- **AI Discover** — natural language search for self-hosted apps via local LLM.
- **Lifecycle** — start, stop, restart, redeploy (compose up --build), remove with dry-run guard.
- **External stack adoption** — detect and manage Docker Compose stacks not deployed by VoidTower.
- **Deploy to Proxmox** — create an LXC on any configured Proxmox host and bootstrap the app inside it.

### Added — Plugin system

- **Plugin registry** — SQLite-backed; plugins stored in `{data_dir}/plugins/{id}/`.
- **Install from URL** — download a `.zip`, extract, read `plugin.json` manifest; handles top-level subdirectory prefix.
- **Static serving** — `/plugin-assets/:id/*` serves plugin files (auth-gated, no restrictive CSP).
- **Iframe host** — each plugin gets a full-page iframe at `/plugins/:id`; `sandbox="allow-scripts allow-same-origin allow-forms"`.
- **Dynamic sidebar** — enabled plugins appear under a "Plugins" section in the nav.
- **`plugin.json` manifest** — `id`, `name`, `version`, `description`, `author`, `entry`, `icon`, `nav_group`.

### Added — Infrastructure management

- **Services** — systemd service list, status, start/stop/restart, logs, policy enforcement for API tokens.
- **Containers** — Docker container list, actions, live log stream WebSocket, exec shell, compose view/edit/propose/apply, policy enforcement.
- **VMs** — local KVM management via libvirt.
- **Storage** — block devices, mounts, fstab editor, SMART data, RAID status/create/stop, format, configurable paths.
- **Network** — LAN neighbor scan.
- **WireGuard** — peer management (requires `wireguard` capability).
- **Firewall** — UFW rule management with dry-run guard (requires `ufw` capability).
- **Proxies** — nginx reverse proxy config CRUD; Docker nginx-proxy container management; embed proxy auto-creation.
- **Files** — file browser with read, write, mkdir, delete, rename, raw serve.
- **Terminal** — local PTY shell + SSH sessions; xterm.js frontend; multi-tab; clipboard; fish/bash/zsh support.
- **Backups** — restic backup configs; scheduled jobs; restore-test runner with confidence widget; daily alert for untested backups.
- **Updates** — VoidTower self-update (bare-metal + Docker), Odysseus update, OS package updates, Docker image updates.

### Added — Observability

- **Dashboard** — CPU, RAM, swap, disk, load average, GPU (NVIDIA/AMD/Vulkan), process count, OS info, uptime.
- **Metrics** — live WebSocket stream; broadcast channel for internal consumers.
- **Alerts** — threshold-based (CPU >85/95%, RAM >80/92%, disk >80/92%); status check alerts; VM state-change alerts; backup never-tested alert. Acknowledge, resolve, delete.
- **Timeline** — global audit-style event log across all resource types.
- **Status checks** — HTTP/TCP uptime checks on a configurable interval; public status page at `/status`.

### Added — Organisation & UX

- **Resource tags** — create, assign, filter across containers, services, apps, proxies, backups, VMs, Proxmox VMs. Global tag filter in TopBar.
- **Nav customisation** — drag-to-reorder nav groups and items; inline group rename; capability-gated items hidden when feature not detected.
- **Themes** — built-in theme library + custom theme builder; transparency and panel rounding sliders; import/export; live preview.
- **Void Mode (AIOS)** — spatial windowing OS shell with 21 native panels; Odysseus AI assistant pane; split-pane layout; device-tier detection.
- **Instance branding** — custom instance name (title bar + sidebar), custom CSS injection, favicon swap.
- **Automation** — cron-style scheduled jobs; manual run; run history; webhook trigger.
- **Integrations** — API token CRUD with scopes; Odysseus config; SSE event stream; notification webhooks (ntfy, Discord, Slack).
- **Models** — llama.cpp model download + load/unload; Ollama pull, create, delete.
- **Mods** — git-based code patch system for community extensions.
- **Capabilities** — auto-detect host features (Docker, systemd, KVM, WireGuard, UFW, etc.); capability page at `/capabilities`.
- **Settings** — general (instance name, logo, custom CSS), AI URL, notification, nav config, branding.

### Changed

- Error messages from Proxmox now surface to the frontend (previously showed generic "internal error").
- Installer bind address defaults to `0.0.0.0`; shows detected LAN IP in post-install summary.
- App embed proxies use port-based nginx configs — no DNS or server_name needed.

### Infrastructure

- Backend: Rust, axum 0.7, sqlx/SQLite, tokio
- Frontend: React 18, TypeScript, Vite, Zustand, Tailwind
- AI: Odysseus (Python/uvicorn) on :7000
- Terminal: portable-pty + xterm.js over WebSocket
- Auth: session cookie `vt_session`; bearer token auto-session injection
- CI: `cargo clippy --all-targets --all-features -- -D warnings` + `npm run build`

---

## [0.1.0] — initial

Initial commit. Proof-of-concept with Ollama model detection, basic dashboard, and early installer scaffold.
