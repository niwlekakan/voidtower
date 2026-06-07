You are an expert autonomous AI software engineer, full-stack systems architect, DevOps engineer, Linux packaging specialist, security engineer, and product designer.

Build a complete, working, clean-room, fully open-source, self-hostable Linux infrastructure management platform named:

# VoidTower

VoidTower is a dark, edgy, technical infrastructure command tower for Linux servers. It is a self-hosted control plane for homelabs, sysadmins, small teams, bare-metal servers, containers, virtual machines, services, backups, status pages, automation, and multi-node infrastructure.

Do not copy code, branding, assets, UI text, trademarks, or proprietary logic from WolfStack, Proxmox, Portainer, Cockpit, Uptime Kuma, Netdata, or any other existing project. This must be a clean-room implementation inspired only by the general category of self-hosted infrastructure dashboards.

The final result must be a real runnable repository, not a mockup.

====================================================================
CORE PRODUCT
====================================================================

VoidTower should let an operator:

- Monitor Linux servers in real time.
- Manage systemd services.
- Manage Docker containers.
- Manage Docker Compose applications.
- Manage LXC containers where available.
- Manage KVM/libvirt virtual machines where available.
- Open a secure browser terminal.
- View logs.
- Manage storage and mounts.
- Configure backups.
- Create status pages.
- Configure alerts.
- Run automation jobs.
- Add other Linux nodes in agent mode.
- Control everything from a dark, fast, technical web UI.

VoidTower must be:

- Fully self-hostable.
- Fully open source.
- Local-first.
- Privacy-respecting.
- Useful on a single Linux server.
- Expandable to many nodes.
- Safe by default.
- Installable with one command.
- Uninstallable cleanly.
- No telemetry.
- No ads.
- No tracking.
- No license server.
- No activation.
- No paid feature gates.
- No cloud dependency.

License:

- Use AGPL-3.0-or-later.
- Include LICENSE, NOTICE, SECURITY.md, CONTRIBUTING.md, CODE_OF_CONDUCT.md, and README.md.
- No noncommercial restrictions.
- No proprietary extensions.

====================================================================
BRAND AND DESIGN
====================================================================

Product name:

VoidTower

Tone:

Dark, sharp, technical, cyber-ops, terminal-native, powerful, slightly ominous, but not childish.

Tagline:

“Command the stack.”

Design language:

- Dark-first UI.
- Cyberpunk infrastructure command center.
- Dense but readable layout.
- Fast operator-focused workflows.
- Thin borders.
- Sharp panels.
- Minimal fluff.
- Command-console motifs.
- Sparse glow effects.
- No soft pastel SaaS look.
- No cartoon mascots.

Theme tokens:

--bg-root: #050509;
--bg-panel: #0b0d14;
--bg-card: #11131d;
--bg-elevated: #171a27;
--border-subtle: #25283a;
--text-primary: #f4f7ff;
--text-secondary: #a8b0c3;
--text-muted: #687086;
--accent-primary: #8b5cf6;
--accent-secondary: #06b6d4;
--accent-success: #39ff88;
--accent-warning: #f59e0b;
--accent-danger: #ef4444;
--terminal-green: #00ff9c;
--terminal-bg: #020403;

UI sections:

- Command
- Nodes
- Containers
- Virtual Machines
- App Vault
- Storage
- Network
- Services
- Terminal
- Backups
- Status
- Automation
- Alerts
- Security
- Settings

Include:

- Responsive layout.
- Dark UI by default.
- Login screen.
- Dashboard.
- Sidebar.
- Top command/search bar.
- Command palette.
- Real-time metric cards.
- Status badges.
- Alert indicators.
- Terminal-like styling where appropriate.

Keyboard shortcuts:

- `/` opens search/command palette.
- `g d` dashboard.
- `g n` nodes.
- `g c` containers.
- `g v` virtual machines.
- `g t` terminal.
- `g s` settings.

====================================================================
WEB UI DESIGN PLAN
====================================================================

VoidTower’s web UI is a first-class product, not an afterthought. The interface must feel like a dark infrastructure command center: fast, dense, sharp, technical, and powerful. It should be usable for real administration work, not just look pretty in screenshots.

The UI must be fully responsive and work well on:

- Desktop monitors.
- Laptops.
- Tablets.
- Mobile browsers (phone).
- PWA/mobile install mode.
- TV / large display (10-foot UI, D-pad navigation).
- Kiosk / wall panel (passive read mode, auto-cycle, PIN wake).

The UI must prioritize:

- Speed.
- Clarity.
- Low visual noise.
- High information density.
- Keyboard navigation.
- Fast search.
- Real-time feedback.
- Clear danger states.
- Operator trust.

The UI must avoid:

- Generic SaaS dashboard styling.
- Excessive whitespace.
- Cartoon mascots.
- Soft pastel colors.
- Unnecessary animations.
- Hidden critical controls.
- Marketing copy inside operational screens.

--------------------------------------------------------------------
TWO UI MODES
--------------------------------------------------------------------

VoidTower ships two distinct UI modes that the operator can switch between at any time using Ctrl+Shift+V or the mode toggle pill in the status bar.

1. Tower Mode (traditional)
   The classic sidebar-plus-content layout. A fixed left navigation sidebar, a top command bar, and a full-width main content panel. This is the default for operators who prefer a conventional admin dashboard feel.

2. Void Mode (AI OS / floating panels)
   A windowed desktop environment that runs inside the browser. Pages open as draggable, resizable, snappable floating panels on a canvas. Multiple panels can be open at once. Four virtual workspaces allow organizing panels into groups. A persistent status bar sits at the top; a floating dock or vertical icon strip sits at the side or bottom depending on screen size. An AI command bar (⌘K / Ctrl+K) replaces the top bar and serves as the main navigation surface. This mode is designed for power operators and AI-first workflows where multiple contexts are open simultaneously.

Both modes share the same page components and backend data. The mode is persisted per-user in local storage and applied immediately on load without a page reload.

--------------------------------------------------------------------
VOID MODE — FLOATING PANEL SYSTEM
--------------------------------------------------------------------

The Void Mode panel system is the AI OS layer of VoidTower.

Panel lifecycle:

- Each page (Dashboard, Containers, Terminal, etc.) opens as a floating panel.
- Panels can be opened from the dock, the command bar, or the command palette.
- Multiple panels can be open on the same workspace.
- Each panel has: title bar, minimize, maximize, close, pin controls.
- Panels are draggable by their title bar.
- Panels are resizable from all edges and corners.
- Panels remember their position and size across sessions.
- Panels have a z-index stack — clicking a panel brings it to front.
- Panels can be pinned to stay on top.

Panel layout modes (snap zones):

- floating (free position)
- left-half / right-half
- top-half / bottom-half
- top-left / top-right / bottom-left / bottom-right (quarter snap)
- fullscreen
- minimized (collapsed to dock with indicator dot)
- sheet (full-width bottom sheet on phone)
- tile (future: auto-tiling layout mode)

Snapping is triggered by:
- Dragging a panel to a screen edge until a snap preview appears.
- Keyboard: Ctrl+Shift+Arrow to snap the focused panel.
- Two panels snapped left-half and right-half auto-couple into a split pair with a draggable divider.

Panel caps by device tier:
- phone: 1 visible panel (sheet mode)
- tablet: 3 panels
- desktop: 5 panels
- large (≥1920px): 8 panels
- tv/kiosk: 5 panels

When a new panel would exceed the cap, the oldest non-pinned visible panel is minimized automatically.

Virtual workspaces:

- Four workspaces (0–3), switched with Ctrl+1/2/3/4.
- Each workspace has its own panel set.
- Panels can be sent to a different workspace via the title bar context menu.
- Workspace dots are shown in the status bar; active workspace is indicated visually.

Embed panels:

- Any http(s) URL typed into the command bar opens as an iframe embed panel.
- This allows embedding self-hosted apps (Gitea, Grafana, Jellyfin, etc.) as panels alongside VoidTower pages.
- Embed panels share the same floating/snap/workspace system.
- Sandbox policy: allow-scripts, allow-same-origin, allow-forms, allow-popups.
- Future: Odysseus integration opens as an embed panel type, allowing the AI chat to sit alongside any infrastructure page.

--------------------------------------------------------------------
VOID MODE — STATUS BAR
--------------------------------------------------------------------

A slim persistent bar fixed at the top of the screen. Height: 28px (desktop), 52px (TV).

Left section:
- VoidTower logo mark (⬡) and name.

Center section (desktop/large/tablet):
- Live metric pills: CPU%, RAM%, NET ↓/↑, GPU%, uptime.
- Colors warn at >60% (amber) and >85% (red).
- Shows “loading…” or “offline” if metrics stream is disconnected.

Center section (phone):
- Workspace dots (active workspace selector).

Right section:
- Workspace dots (desktop — phone shows them center).
- Split exit button (shown when a split pair is active).
- WebSocket connection indicator (Wifi / WifiOff icon).
- Notification bell (with count badge, dropdown list).
- Clock (HH:MM, tabular numerals, 24h).
- UI Mode Toggle pill (switches between Tower and Void modes).

--------------------------------------------------------------------
VOID MODE — DOCK
--------------------------------------------------------------------

The dock is the primary navigation surface in Void Mode.

Layout by device tier:

- Desktop/large (≥1400px width): vertical icon strip fixed to the left edge, scrollable.
- Desktop/tablet (<1400px): centered horizontal pill floating above the bottom of the screen.
- Phone: full-width bottom tab bar with icon + label, fixed to the bottom edge.
- TV/kiosk: hidden (these tiers use dedicated layouts).

Dock items (current set):
Dashboard, Alerts, Timeline, Services, Containers, VMs, App Vault, AI, Models, Network, Proxies, WireGuard, Firewall, Storage, Backups, Files, Security, Secrets, Audit Log, Automation, Terminal, Capabilities, Diagnostics, Themes, Updates, Mods, Integrations, Tags, Settings.

Dock item states:

- Default: muted icon.
- Open (panel visible): accent color, outline ring, indicator dot below icon.
- Minimized: amber indicator dot.

Clicking a dock item:
1. If a minimized panel for that key exists on this workspace: restore and focus it.
2. If an open panel for that key exists: focus it.
3. Otherwise: open a new panel via the openApp callback (which uses defaultGeometry based on tier and existing panel count).

Tooltips appear on hover (desktop only) positioned to the right of a vertical dock or above a horizontal dock.

--------------------------------------------------------------------
VOID MODE — COMMAND BAR
--------------------------------------------------------------------

A floating pill input fixed above the dock, centered horizontally.

Shortcut: ⌘K / Ctrl+K to focus. Escape to dismiss.

Input modes:

1. App search (default): fuzzy matches dock items by label or key. Shows up to 8 results with icons. Arrow keys navigate, Enter opens.
2. Odysseus mode: prefix with `/`. Shows “Ask Odysseus” row. Enter routes the query to the active Odysseus panel via postMessage, or opens the AI page. Prompt: `/why is jellyfin using 95% CPU`.
3. URL embed mode: prefix with http:// or https://. Shows “Open as embed” row. Enter opens the URL as an iframe embed panel.

Phone: the command bar is a FAB (floating action button, search icon) fixed above the dock. Tapping opens a bottom sheet with the same three input modes.

TV/kiosk: command bar is hidden (navigation uses dedicated layouts).

--------------------------------------------------------------------
VOID MODE — TV LAYOUT
--------------------------------------------------------------------

Used on coarse-pointer large screens (TV, monitor without mouse).

A 3×2 (or 2×2 for ≤4 tiles) grid of large icon+label tiles. Default tiles: Dashboard, Alerts, Containers, Services, AI, App Vault, Terminal, Network, Backups, Storage, Security, Diagnostics.

Navigation: Arrow keys move focus. Enter opens the selected tile as a fullscreen panel. Escape goes back to the grid.

A Back button is shown in the expanded view.

The status bar is present at the top (larger, 52px).

--------------------------------------------------------------------
VOID MODE — KIOSK LAYOUT
--------------------------------------------------------------------

Used via `?mode=kiosk` URL parameter. Intended for wall panels, NOC displays, and public status boards.

Behavior:

- Shows a configurable set of tiles (default: dashboard, containers, alerts) in a grid.
- Auto-cycles through tiles on a configurable interval (default: 30s).
- Passive read mode: the UI animates tile highlights but does not interact.
- After IDLE_SCREENSAVER_MS (10 min) of no interaction: dims to 50% opacity and shows a clock screensaver.
- Tap or click anywhere to wake from screensaver.
- Optional PIN: if configured, tapping shows a 4-digit PIN pad before unlocking interactive mode.
- Interactive mode: lasts 5 minutes, then reverts to passive cycle mode. During interactive mode, clicking a tile opens it.
- Critical alert flash: polls /api/alerts every 60s. If any critical alert is active, flashes a red border outline for 5s.

Kiosk config stored in localStorage under `kiosk_layout`:

```json
{
  “tiles”: [“dashboard”, “containers”, “alerts”],
  “cycleInterval”: 30000,
  “wakePin”: “”
}
```

--------------------------------------------------------------------
TOWER MODE — LAYOUT
--------------------------------------------------------------------

Tower Mode uses a conventional sidebar-plus-content layout.

1. Left sidebar
   - VoidTower product mark.
   - Navigation groups (collapsible sections).
   - Health badges on sections with active alerts.
   - Collapse/expand toggle.
   - User/account controls at the bottom.
   - On mobile: becomes a slide-out drawer triggered by a hamburger button.

2. Top bar
   - Page title and breadcrumbs.
   - Global search input.
   - Command palette trigger (⌘K).
   - Alert count indicator.
   - UI Mode Toggle (switch to Void Mode).
   - Current user menu (profile, settings, logout).

3. Main content panel
   - Page-specific content: cards, tables, charts, forms, terminal panels, split views.
   - Full-width on desktop, responsive on tablet/mobile.

4. Right context drawer (future)
   - Slide-in detail panel for a selected entity (container, service, VM).
   - Recent logs, related actions, audit history.
   - “Send to Odysseus” button.

5. Bottom task console (future)
   - Expandable strip for background jobs, automation runs, deployment progress, backup progress.
   - Expandable log output.

--------------------------------------------------------------------
NAVIGATION STRUCTURE
--------------------------------------------------------------------

Current navigation sections (both Tower sidebar and Void dock):

Infrastructure:
- Dashboard
- Services
- Containers
- VMs
- App Vault
- Storage
- Files

Network:
- Network
- Proxies
- WireGuard
- Firewall

Operations:
- Backups
- Automation
- Alerts
- Timeline
- Terminal

AI / Intelligence:
- AI (Odysseus workspace embed + recommendations)
- Models (GGUF / Ollama model manager)

System:
- Capabilities
- Diagnostics
- Secrets
- Security
- Audit Log
- Tags
- Integrations
- Updates
- Mods (plugins, future)
- Themes
- Settings

Navigation requirements:

- Tower sidebar must be collapsible to icon-only mode.
- Sections with active warnings must show alert count badges.
- Current section must be visually obvious (active highlight, accent color).
- Navigation must support keyboard shortcuts.
- Tower mode must include breadcrumbs for deeper views.
- Mobile Tower sidebar becomes a slide-out drawer.
- Void dock auto-adapts to device tier (vertical strip / horizontal pill / bottom tab bar).

Keyboard shortcuts:

- `Ctrl+K` or `Cmd+K` opens command bar / command palette.
- `Ctrl+Shift+V` toggles between Tower and Void modes.
- `Ctrl+1` / `Ctrl+2` / `Ctrl+3` / `Ctrl+4` switches workspaces (Void Mode).
- `Ctrl+Shift+Arrow` snaps focused panel to a zone (Void Mode).
- `Ctrl+W` closes focused panel (Void Mode).
- `Ctrl+M` closes all panels (Void Mode).
- `Alt+Tab` cycles visible panels on current workspace (Void Mode).
- `Escape` minimizes focused panel or closes modal/drawer.
- `?` opens keyboard shortcut help.

--------------------------------------------------------------------
COMMAND PALETTE
--------------------------------------------------------------------

VoidTower includes a command palette (Ctrl+K) in both UI modes.

In Tower Mode, the command palette is a modal overlay.
In Void Mode, the command palette is integrated into the command bar.

The command palette allows users to:

- Navigate to any page.
- Open apps in Void Mode panels.
- Open embed URLs as panels.
- Send Odysseus queries (prefix: /).
- Search containers, services, VMs by name.
- Trigger automations (if RBAC allows).
- Open a terminal session.
- Run safe system actions.

Dangerous actions must never execute directly from the palette. They must navigate to the relevant page where the confirmation flow lives.

Examples:

- “Containers” → opens Containers page/panel.
- “Terminal” → opens Terminal panel.
- “Backups” → opens Backups page.
- “/why is nginx failing” → routes to Odysseus with that query.
- “https://gitea.local” → opens as embed panel (Void Mode).

--------------------------------------------------------------------
DASHBOARD / COMMAND PAGE
--------------------------------------------------------------------

The Dashboard is the main landing page in Tower Mode and the default first panel in Void Mode.

Current implementation: customizable widget grid with drag-to-reorder. Widgets include CPU/RAM/disk/network charts, container summary, alert count, and clock.

The Dashboard should show:

- System health overview (CPU, RAM, disk, network).
- Active alert count (with severity breakdown).
- Failed or degraded services count.
- Unhealthy containers count.
- VM status summary.
- Backup confidence card (last backup time, last restore test, confidence level).
- Recent automation runs.
- Recent timeline/audit events.
- Security warning count.
- Quick action buttons.

Dashboard cards must support:

- Loading state.
- Empty state.
- Healthy / warning / critical states.
- Click-through to the relevant detail page.

Charts:

- CPU history (sparkline or area).
- RAM history.
- Disk usage per mount point.
- Network RX/TX history.
- GPU utilization (if GPU detected).

Charts must be readable in dark mode. Glow effects must be restrained.

Future dashboard additions:
- Backup confidence widget with scheduled restore test status.
- Per-node health grid (multi-node mode).
- Config drift alerts widget.
- Maintenance window banner.
- Incident mode banner when an active incident exists.
- AI insight card: Odysseus-generated one-line infrastructure summary (opt-in).

--------------------------------------------------------------------
TABLES AND DATA GRIDS
--------------------------------------------------------------------

VoidTower displays many operational tables. Tables must be excellent.

Requirements:

- Sortable columns.
- Filter/search.
- Status badges.
- Bulk selection where safe.
- Row actions (inline buttons or dropdown).
- Expandable row details.
- Column visibility controls.
- Compact and comfortable density modes.
- Pagination or virtual scrolling for large datasets.
- Copy-to-clipboard for IDs, IPs, paths, commands, tokens.
- Clear empty states.
- Clear loading states.
- Clear error states.

Tables needed for (current):

- Containers (Docker).
- Images.
- Volumes.
- Networks.
- VMs (libvirt / Proxmox).
- Services (systemd units).
- Backups / snapshots.
- Alerts.
- Automations.
- Audit log.
- API tokens.
- Integrations / Odysseus events.
- Secrets.
- Firewall rules.
- Proxy rules.
- WireGuard peers.
- Storage block devices.
- Status checks.

Future table additions:
- Multi-node: nodes table with per-node health columns.
- Inventory / asset database (hardware, OS, packages per node).
- Plugin registry.
- Policy rules (policy engine, future).
- Maintenance windows.
- Incidents.

--------------------------------------------------------------------
DETAIL PAGES
--------------------------------------------------------------------

Major objects should have detail pages or expandable drawers.

Currently built:

- Container detail page (ContainerDetail.tsx): full stats, logs, exec, compose diff.

Required for future:

- Node detail: hardware summary, all metrics history, running services, running containers, open ports, recent events, audit history, notes, Send to Odysseus button.
- VM detail: config, metrics, snapshots, console link, lifecycle actions, danger zone.
- Service detail: status, logs, unit file view, start/stop/restart, override editor.
- App detail (App Vault): deployed compose, env vars, logs, update, redeploy, rollback.
- Backup detail: snapshot list, integrity check result, last restore test, restore button.
- Alert detail: timeline, related metrics at alert time, acknowledge/resolve/silence.
- Automation detail: run history with per-run log output, edit, dry-run, manual trigger.

Detail page structure:

- Summary header with name, status badge, key metadata.
- Metrics section (where applicable).
- Logs section (where applicable).
- Related resources.
- Recent audit events.
- Safe action buttons.
- Danger Zone: grouped destructive actions, always separated visually, typed-name confirmation for irreversible operations.

--------------------------------------------------------------------
TERMINAL UI
--------------------------------------------------------------------

Terminal is a central part of VoidTower.

Current implementation: full PTY browser terminal with shell auto-detection and SSH session manager (Terminal.tsx).

Requirements:

- xterm.js-based terminal.
- Full-screen mode (fills the panel or the full window).
- Node selector (host shell vs. container exec).
- Container exec selector.
- SSH session manager: save/load saved sessions.
- Font size controls.
- Copy/paste support.
- Session status indicator.
- Reconnect handling with clear disconnect state.
- Optional session recording indicator.
- Audit notice when session recording is active.

Terminal design:

- Background: near-black.
- Text: terminal green by default (theme-controlled).
- Cursor: bright accent.
- Minimal chrome: only title bar with session metadata.
- Session metadata shown: node, user, shell, started time, recording state.

In Void Mode, the Terminal opens as a panel that can be snapped, split, or resized like any other panel. Two terminal panels side-by-side (left-half + right-half snap) gives a native split-terminal experience.

Future:
- Named session groups / tab bar within one terminal panel.
- Session replay viewer (if recording is stored).

--------------------------------------------------------------------
LOG VIEWER
--------------------------------------------------------------------

Logs must be easy to inspect.

Current implementation: LogViewer.tsx component used across Services, Containers, Backups, Automation.

Requirements:

- Live tail mode.
- Pause/resume.
- Search within logs.
- Regex filter.
- Severity highlighting (error/warn/info/debug color coding).
- Timestamp normalization.
- Download logs.
- Copy selected lines.
- Wrap/nowrap toggle.
- Follow mode (auto-scroll to bottom).
- Jump to bottom button.
- Redact known secret patterns where possible.

Log viewer is used for:

- systemd services.
- Docker containers.
- VMs (where journald or serial log is available).
- Restic backup runs.
- Automation job runs.
- Installer logs.
- Audit event detail.
- Odysseus-triggered actions (future).

Future:
- Structured log parsing (JSON logs rendered as expandable key-value rows).
- Correlation: click a log line timestamp to jump to the timeline at that moment.
- Export as file (JSON, plain text, filtered subset).

--------------------------------------------------------------------
APP VAULT UI
--------------------------------------------------------------------

App Vault is the application deployment area.

Current implementation: 40+ one-click deployments, management panel per app with Containers/Compose/Logs tabs, AI-based app recommendation, iframe embed support (AppVault.tsx).

Design:

- Dark catalog grid.
- App cards with icon, name, short description, category badge.
- Category filters (self-hosted, dev, media, communication, AI, network, productivity, etc.).
- Search by name or description.
- Badges for official/community/local templates.
- Deployment wizard (see below).
- Compose preview before deploy.
- Environment variable editor.
- Volume/port editor.
- Validation before deploy.
- Deployment progress log.
- Rollback info where available.
- AI recommendation chip on cards (“Odysseus suggests this for your setup”).

App card badges (planned — see future_plan.md §21):

- AI Native — Odysseus has full tool coverage for this app.
- AI Aware — Odysseus can read status and logs but cannot act.
- AI Ready — no integration yet, template exists for one-click wiring.
- (none) — community app, unknown AI integration.

Badge color: cyan = native, blue = aware, grey outline = ready.

Deployment wizard steps:

1. Select app.
2. Choose target node (once multi-node exists; currently: local only).
3. Configure environment variables.
4. Configure ports and volume mounts.
5. Preview generated Docker Compose file.
6. Review change plan (dry-run, once implemented).
7. Confirm deployment.
8. Watch deployment logs in real time.

Custom app deployment (planned):
- A “Deploy custom” button opens a minimal form: image, name, port maps, volume maps, env vars.
- VoidTower generates and saves a compose file and deploys it.
- The deployed app appears in the running apps list like any App Vault deployment.

App management panel (per deployed app):
- Containers tab: list containers, start/stop/restart/remove.
- Compose tab: view and edit the generated compose file with staged diff preview.
- Logs tab: live log tail across all containers in the compose project.
- Actions: update (pull latest image), redeploy, rollback (if rollback point exists), remove.

--------------------------------------------------------------------
AUTOMATION UI
--------------------------------------------------------------------

Automation must feel powerful but safe.

Current implementation: cron-style shell jobs with run history and output, enable/disable (Automation.tsx). Editor is a shell command + cron expression.

Views:

- Automation list (name, schedule, last run, last status, enable/disable toggle).
- Automation editor.
- Run history with per-run log output.
- Manual run button.
- Odysseus invocation history (once policy engine and action linking are implemented).

Current editor:

- Shell command input.
- Cron expression input.
- Enable/disable toggle.
- Save.

Planned editor additions:

- Dry-run button: shows what the command would do without executing.
- Secrets picker: insert a secret reference without exposing plaintext.
- Schema-aware YAML editor (once automation engine supports multi-step definitions).

Future editor (visual workflow):

- Drag-and-drop trigger/action graph.
- Conditional branches (if/else on exit code or output).
- Multi-step sequences.
- Built-in action types: restart service, stop container, run backup, send notification, HTTP request, delay.
- Run preview: show execution path before running.

--------------------------------------------------------------------
ALERTS UI
--------------------------------------------------------------------

Alerts must be impossible to miss but not obnoxious.

Current implementation: metric threshold alerts + TCP/HTTP status checks, ack/resolve, public /status page (Alerts.tsx).

Alert states:

- Info.
- Warning.
- Critical.
- Resolved.
- Acknowledged.
- Silenced.

Alert page must support:

- Filtering by severity, node, category.
- Acknowledge (suppresses notification, keeps alert visible).
- Silence (hides from active list for a duration).
- Resolve.
- Assign owner (if users exist).
- Send to Odysseus (SendToOdysseus component — present in codebase, copies context to clipboard and opens Odysseus URL).
- View related logs.
- View related timeline entry.

Critical alerts appear in:

- Void Mode status bar: notification bell badge.
- Kiosk mode: red border flash.
- Tower Mode sidebar badge on Alerts section.
- Dashboard alert count card.
- Alerts page.

Future alert features:
- Maintenance window suppression: alerts generated during a configured window are silenced automatically.
- Incident creation: “Open incident from this alert” button.
- Alert routing by tag: route alerts tagged `prod` to one webhook, `lab` to another.
- Alert grouping: collapse repeated same-source alerts into one entry with a count.

--------------------------------------------------------------------
SECURITY UI
--------------------------------------------------------------------

Security section must be blunt and useful.

Current implementation: session list for all users, revoke individual/all-other sessions, full audit log (Security.tsx). Secrets managed in Secrets.tsx.

Views:

- Security overview (summary of active sessions, recent login attempts, open findings).
- File permission scanner.
- Exposed services (open ports vs. firewall rules).
- Login attempts (rate limiting events).
- Active sessions (all users, revoke button).
- API tokens (list, create, revoke, scope display).
- Odysseus integration access (enabled/disabled, token scope, last used).
- Audit log (full operation history).
- Secrets manager (AES-256-GCM encrypted store, reveal-on-demand with audit log).
- TLS/certificate status (expiry dates, renewal state).
- Dangerous capability review (which capabilities are enabled that increase attack surface).

Security findings should be grouped by severity:

- Critical.
- High.
- Medium.
- Low.
- Info.

Every finding should include:

- What was detected.
- Why it matters.
- Recommended fix.
- Whether VoidTower can fix it automatically.
- Manual command suggestion where safe.

Future security additions:
- TOTP enrollment UI (backend totp.rs module exists but no frontend page/flow yet).
- WebAuthn / passkey registration and login.
- Emergency disable panel: one-click buttons to disable Odysseus access, all automations, all webhooks, MCP server.
- Policy engine UI: define per-actor rules for what actions are allowed on which resources.

--------------------------------------------------------------------
SETTINGS UI
--------------------------------------------------------------------

Settings must be comprehensive but organized.

Current implementation: Settings.tsx with multiple sections.

Settings sections (current):

- General (bind, port, data paths)
- Appearance (theme selection, animated background, glass level)
- Theme Editor (full live token editor)
- Users
- Roles & Permissions
- Authentication (password policy)
- Sessions
- API Tokens
- Integrations (Odysseus config, MCP toggle, webhook secret, SSE events)
- Notifications
- Alerts
- Backups
- Network
- TLS
- App Vault
- Automation
- Security
- Diagnostics
- Advanced
- About

Settings must include search.

Settings changes must show:

- Unsaved state indicator.
- Validation errors inline.
- Reset/revert option.
- Save confirmation for sensitive changes.
- Audit log entry for sensitive settings changes.

Future settings additions:

- Plugin manager section (once plugin system is built).
- Policy engine section (actor/resource/action rules).
- Maintenance windows section.
- Disaster recovery section (export config, import config, emergency reset).
- Node management section (once multi-node agent mode is built).
- TOTP / WebAuthn section under Authentication.

--------------------------------------------------------------------
FULL THEME CUSTOMIZATION
--------------------------------------------------------------------

VoidTower allows full theme customization from inside the web UI.

Current implementation: 7 built-in themes, live custom CSS token editor with 14+ animation parameters, animated background system with 7 canvas presets (Void, Grid, Aurora, Pulse, Noise, Hex, Circuit) and 4 glass levels (Themes.tsx, ThemeEditor.tsx, ThemeProvider.tsx).

The Theme Editor allows customization of:

- Base mode: dark / darker / light / custom.
- Background colors (root, panel, card, elevated).
- Border colors.
- Text colors (primary, secondary, muted).
- Accent colors (primary, secondary, success, warning, danger).
- Terminal background, foreground, cursor color.
- Chart colors.
- Font family.
- Font size scale.
- UI density: compact / normal / comfortable.
- Border radius: sharp / slight / rounded.
- Glow intensity: off / low / medium / high.
- Animation level: off / reduced / normal.
- Table density.
- Terminal font.
- Code/log font.
- Card shadow depth.
- Sidebar width (Tower Mode).
- Animated background preset and intensity.
- Glass blur level (0–4).

Theme requirements:

- Themes stored locally (Zustand + localStorage).
- Themes exportable as JSON.
- Themes importable from JSON.
- Users can duplicate a theme.
- Users can reset to defaults.
- Users can preview before applying.
- Users can save multiple named themes.
- Admins can set a global default theme.
- Users can override the global theme for their own account.
- Theme changes apply live without page reload (CSS variable injection).
- Invalid colors are rejected.
- Accessible contrast warnings are shown.
- Theme editor must never allow unsafe CSS injection.

Built-in themes (current):

1. VoidTower Default — dark cyber-ops, violet/cyan accents.
2. Blacksite — near-black, red danger accents, minimal glow.
3. Ghost Terminal — black/green terminal-inspired.
4. Deep Grid — indigo/cyan infrastructure-grid.
5. Solar Breach — dark amber/orange operations.
6. Light Ops — light theme for daylight.
7. High Contrast — accessibility-first.

Theme implementation:

- CSS variables (var(--bg-root), var(--accent-primary), etc.).
- Store tokens in database for cross-device sync (future) or localStorage for local-only.
- Apply at login and on live change.
- Expose current theme at `/api/settings/theme`.

Theme JSON schema example:

```json
{
  “name”: “Ghost Terminal”,
  “mode”: “dark”,
  “tokens”: {
    “bgRoot”: “#020403”,
    “bgPanel”: “#050806”,
    “bgCard”: “#08110c”,
    “textPrimary”: “#d7ffe8”,
    “textSecondary”: “#7cffb2”,
    “accentPrimary”: “#00ff9c”,
    “accentSecondary”: “#00b8ff”,
    “accentDanger”: “#ff3355”,
    “borderSubtle”: “#123022”,
    “terminalBg”: “#000000”,
    “terminalFg”: “#00ff9c”,
    “terminalCursor”: “#ffffff”
  },
  “density”: “compact”,
  “radius”: “slight”,
  “glow”: “medium”,
  “animations”: “reduced”
}
```

--------------------------------------------------------------------
ACCESSIBILITY
--------------------------------------------------------------------

VoidTower must be accessible enough for serious daily use.

Requirements:

- Keyboard navigable throughout.
- Visible focus states (outline ring on interactive elements).
- Semantic HTML (buttons are buttons, not divs).
- ARIA labels on icon-only buttons, modals, and live regions.
- Sufficient color contrast (WCAG AA minimum).
- High Contrast built-in theme.
- Reduced motion support (respects prefers-reduced-motion; animations level “off” disables all transitions).
- Screen-reader-friendly status updates where practical (aria-live).
- Do not rely on color alone for state (use icon + color, or label + color).
- Icons must have labels or tooltips.
- Theme editor must warn if custom color choices create poor contrast ratios.
- Kiosk mode wake hint and PIN pad must be keyboard accessible.
- Void Mode panel controls (minimize, maximize, close) must have aria-labels.

--------------------------------------------------------------------
RESPONSIVE / MOBILE UI
--------------------------------------------------------------------

Mobile UI is handled via the device tier system, not via CSS-only media queries.

Device tiers and their UI adaptations:

- phone (<640px): Void Mode uses sheet panels (full-width bottom sheet, one at a time). Dock is a full-width bottom tab bar with icons + labels. Command bar is a FAB above the dock. Status bar shows workspace dots in center. Tower Mode sidebar becomes a slide-out drawer.
- tablet (640–1199px): Void Mode uses up to 3 floating panels, centered pill dock. Tower Mode uses a collapsible sidebar.
- desktop (1200–1919px): Void Mode uses up to 5 floating panels, centered pill dock or vertical strip if width ≥1400.
- large (≥1920px): Void Mode uses up to 8 panels, vertical strip dock.
- tv (coarse pointer + ≥1200px): TV grid layout with D-pad navigation.
- kiosk (URL param ?mode=kiosk): kiosk auto-cycle layout.

Tier is detected from window dimensions, pointer media queries, and an optional localStorage override for manual testing.

Mobile use cases:

- Check and acknowledge alerts.
- Restart a failing service.
- View container logs.
- Run a manual automation job.
- Check backup result.
- Open a terminal session.
- Approve or deny a pending Odysseus action (future: AI approval queue).

--------------------------------------------------------------------
REAL-TIME UX
--------------------------------------------------------------------

The UI must clearly show live state.

Current implementation: WebSocket SSE metrics stream, Zustand metrics store, connection indicator in status bar, toast notification system.

Requirements:

- Real-time metric updates via SSE (no polling).
- Connection status indicator (green Wifi icon = connected, amber WifiOff = disconnected).
- Reconnecting state with backoff (shown in status bar).
- Stale data indicator if metrics have not updated for >30s.
- Last updated timestamp on metric cards.
- Background job progress (deployment, backup, automation run).
- Toast notifications for completed actions (success, warning, error).
- Persistent task log in bottom console for long operations (future).
- Optimistic updates only where safe (e.g., toggle enable/disable).

If the backend connection drops:

- Show degraded/offline banner.
- Metric pills in status bar show “offline”.
- Unsafe actions (start/stop/deploy/delete) are disabled with a tooltip explaining the disconnect.
- No silent queuing. Every action either succeeds or fails loudly.
- Auto-reconnect attempt with exponential backoff.

--------------------------------------------------------------------
CONFIRMATIONS AND DANGER ZONES
--------------------------------------------------------------------

Dangerous actions must use strong confirmation UX.

Actions requiring confirmation:

- Delete container.
- Delete VM.
- Delete volume.
- Delete backup snapshot.
- Purge data directory.
- Modify firewall rules.
- Expose service publicly.
- Rotate secrets.
- Run arbitrary shell command.
- Disable authentication.
- Enable MCP server or Odysseus high-risk tools.
- Uninstall package.
- Remove node from cluster.
- Reset node identity.
- Emergency disable all AI access.
- Format disk.
- Apply a disaster recovery import (replaces all data).

Confirmation dialog must show:

- Exact target name.
- Consequence description.
- Whether the action is reversible.
- Required permission level.
- A note that the action will be audit logged.
- Optional typed confirmation for irreversible actions (“Type prod-db-01 to confirm”).

Danger Zone design:

- Grouped at the bottom of detail pages and settings sections.
- Visually separated by a red-bordered section with a “Danger Zone” heading.
- Buttons use accent-danger color with extra padding.
- Buttons are disabled until a prerequisite is met (e.g., type the name).

Future additions:
- Dry-run preview before destructive actions: show a change plan (files touched, services restarted, containers removed, ports affected) before execution.
- Rollback point creation prompt: “VoidTower will create a rollback point before this action. Continue?”

--------------------------------------------------------------------
ODYSSEUS UI TOUCHPOINTS
--------------------------------------------------------------------

When Odysseus integration is enabled, the UI includes AI-agent handoff controls.

Current implementation: SendToOdysseus.tsx component (copies context to clipboard + opens Odysseus URL). AiosCommandBar supports `/` prefix to route to Odysseus.

“Send to Odysseus” buttons are present on (or planned for):

- Alerts (send alert context for diagnosis).
- Failed services (send service name + recent logs).
- Containers (send container state + logs).
- VMs (send VM config + status).
- Backup failures (send backup job + error).
- Security findings (send finding description + remediation context).
- Log line selections (send selected text as context).
- Automation failures (send run output + definition).
- Node health pages.

Send-to-Odysseus flow:

1. User clicks “Send to Odysseus” on an entity.
2. VoidTower packages the context (name, status, logs, metadata).
3. Secrets and sensitive env vars are redacted from the context before packaging.
4. Context is copied to clipboard and Odysseus opens (in a new tab, or as an embed panel in Void Mode).
5. Action is logged in the audit trail.

Future Odysseus UI additions:

- AI approval queue: a panel showing pending high-risk AI-requested actions. User can approve once, deny, approve with a time limit, or create a policy from a repeated safe pattern. Every decision is logged.
- Context preview modal: before sending, show exactly what data will be shared and what will be redacted.
- Odysseus panel type: a dedicated embed panel type in Void Mode that maintains Odysseus state and receives prefill queries via postMessage from any panel.
- “Ask Odysseus” quick action in every detail page header.
- Inline AI insight chips: optional one-line AI annotation on resource cards (“Last restart was caused by OOM — consider increasing memory limit”).

--------------------------------------------------------------------
FUTURE UI AREAS (PLANNED)
--------------------------------------------------------------------

The following UI surfaces do not yet exist but are planned or in the backlog. This section captures their intended design so it can guide implementation.

Multi-node / Agent Mode:

- Nodes page: table of all registered nodes with per-node health columns (CPU, RAM, disk, alerts, last seen).
- Node selector: dropdown or sidebar section to scope all pages to a specific node.
- Node detail page: full hardware summary, all metrics, running services/containers, open ports, recent events, notes.
- Node add wizard: enter agent URL + join token, test connection, register.
- Per-node alert routing: alerts tagged with the source node.

Maintenance Windows:

- Maintenance windows page: create/edit windows with start/end time, affected nodes/resources, suppressed alert categories, status page message.
- Active maintenance banner: shown on the dashboard and in the status bar during a window.
- Automation policy during windows: only allow selected automations to run.

Incident Mode:

- Incidents page: create incident from an alert, attach logs/metrics/services/containers.
- Incident detail: timeline view, owner assignment, status tracking (Investigating / Identified / Monitoring / Resolved / Postmortem Pending), notes editor.
- Postmortem export: generate a markdown postmortem from the incident data.
- “Send incident to Odysseus” button: packages the full incident context for AI-assisted investigation.

Config Drift Detection:

- Drift page or section in resource detail pages.
- Shows expected state (what VoidTower last set) vs. actual state (current on disk/system).
- Inline diff view.
- Reconcile button (apply VoidTower’s desired state) or Accept button (adopt the external change as the new baseline).
- Ignore rule for known intentional differences.

Inventory / Asset Database:

- Inventory page: hardware, CPU, RAM, disk, GPU, network interfaces, OS/kernel, installed packages, owners, notes, warranty metadata.
- Queryable: “which nodes have GPUs?”, “which nodes run public-facing services?”, “which services have no backups?”
- Notes field per resource (Markdown, pinnable, searchable).

Declarative / GitOps Mode:

- State export: download current VoidTower state as a YAML file.
- State import: upload a desired-state YAML, preview the diff, apply with rollback point.
- Optional Git sync: connect to a repository, pull desired state on a schedule, apply with dry-run.
- Pull-request-style change flow: AI can draft a state change YAML instead of acting directly on production.

Policy Engine:

- Policy rules page: create rules binding actor (user / role / API token / Odysseus / automation) to allowed actions on resource types, with time window and approval requirements.
- Policy violation log: show when a policy rule blocked or required approval for an action.
- Policy testing: dry-run a hypothetical action to see which rules apply.

Plugin Manager (Mods):

- Mods page (present as Mods.tsx placeholder).
- List installed plugins with name, version, permissions declared, status (enabled/disabled).
- Plugin install from URL or local file (once plugin SDK exists).
- Plugin permission review before enable.
- Per-plugin audit log.
- Plugin detail page: description, declared permissions, registered routes/actions/tools.

Disaster Recovery:

- Disaster recovery section in Settings (or dedicated page).
- Export config: download a full VoidTower config + database backup as an encrypted archive.
- Import config: restore from an archive (requires typed confirmation — replaces all data).
- Emergency admin reset: creates a new Owner account without the web UI.
- Emergency disable panel: individual one-click disable buttons for Odysseus, all automations, all webhooks, MCP server.
- Recovery status page: shown when VoidTower detects a corrupted or incomplete state on startup.

OpenAPI / Developer Tools:

- API docs page: browsable Swagger/Redoc UI served from /api/openapi.json.
- Available at /api/docs once a UI is wired.
- Useful for plugin developers, SDK users, and Odysseus tool authors.

Demo / Simulation Mode:

- Enabled via `voidtower --demo` CLI flag.
- UI shows a “Demo Mode” banner in the status bar.
- All data is synthetic (fake nodes, containers, metrics, alerts, backups, VMs).
- No real host operations are performed.
- Useful for screenshots, documentation, onboarding, and UI development without a real server.

--------------------------------------------------------------------
FRONTEND CODE QUALITY
--------------------------------------------------------------------

Frontend must be maintainable.

Current architecture:

- React + TypeScript + Vite.
- Zustand for all state (auth, metrics, theme, notifications, cmdpalette, embed, aios panels).
- Central API client at src/api/client.ts.
- Central type models at src/api/types.ts.
- Component structure: src/components/layout/, src/components/ui/, src/pages/, src/aios/.
- Two layout modes: AppLayout.tsx (Tower) and AiosLayout.tsx (Void).

Requirements:

- Componentized structure: reusable cards, tables, modals, forms.
- Central API client: no raw fetch() calls outside of src/api/.
- Central auth state: no local auth logic in page components.
- Central theme store: no hardcoded colors outside CSS variables and theme tokens.
- Central notification system: all toasts go through notify.success/warning/error.
- Type-safe API models: responses are typed at the client layer.
- Loading / error / empty states for every data-fetching view.
- No hardcoded API URLs.
- No inline secrets or tokens.
- No telemetry dependencies.
- Panel error boundary: each Void Mode panel is wrapped in PanelErrorBoundary so one broken page does not blank the whole canvas.

Component groups (current):

- Layout: AppLayout, AiosLayout, AiosTvLayout, AiosKioskLayout, Sidebar, TopBar.
- Void OS: AiosPanel, AiosDock, AiosCommandBar, AiosStatusBar, AiosSplitDivider.
- UI primitives: Button, StatusBadge, TagPill, MetricCard, MetricChart, LogViewer, MiniTerminal, ConfirmDialog, NotificationToasts, CommandPalette, ThemeEditor, UiModeToggle, SendToOdysseus, AppEmbedOverlay, AnimatedBackground, ForcePasswordChange.
- Store hooks: useMetrics, useKeyboard, useDeviceTier, useSnapZones, useTouchGestures.

Future component additions:

- DangerZone: standardized danger section wrapper with red border and confirmation gating.
- EntityHeader: reusable detail-page header with name, status badge, key metadata, action row.
- ChangePreview: dry-run change plan display (files, services, commands, rollback state).
- AiInsightChip: inline Odysseus-generated annotation on resource cards.
- AiApprovalQueue: panel listing pending AI-requested high-risk actions.
- MaintenanceBanner: dashboard and status bar banner during a maintenance window.
- IncidentBadge: status badge shown on dashboard/sidebar when an active incident exists.
- PolicyRuleEditor: form for creating/editing policy engine rules.
- DriftDiff: side-by-side diff between expected and actual resource state.
- NoteEditor: Markdown note editor for per-resource notes.
- InventoryCard: hardware summary card for nodes.

--------------------------------------------------------------------
WEB UI ACCEPTANCE CRITERIA
--------------------------------------------------------------------

The web UI is acceptable only if:

- It has a polished dark VoidTower identity in both Tower and Void modes.
- Tower Mode has a working sidebar + topbar + content layout.
- Void Mode has working floating panels, dock, command bar, and status bar.
- Device tiers are detected and the correct layout variant is served (phone / tablet / desktop / large / tv / kiosk).
- Dashboard shows live metrics.
- Navigation works in both modes.
- Command palette / command bar opens with Ctrl+K, supports app navigation, Odysseus queries, and URL embeds.
- Tables are sortable, filterable, and have appropriate empty/loading/error states.
- Container detail page works with logs, exec, and compose diff.
- Terminal opens and connects to a PTY session.
- Loading, empty, error, and offline states are present on every data view.
- Dangerous actions show a typed confirmation dialog.
- Theme Editor is accessible from Settings → Appearance.
- Users can fully customize theme tokens live without page reload.
- Themes can be saved, duplicated, imported, exported, reset, and applied live.
- Theme customization uses validated CSS variables and never allows arbitrary CSS injection.
- Animated backgrounds can be selected and configured.
- Accessibility basics are implemented: keyboard nav, focus states, ARIA labels, no color-only state.
- Mobile (phone tier) layout is usable for alerts, service restart, log view, and terminal.
- Kiosk mode activates via ?mode=kiosk, auto-cycles tiles, and shows a screensaver.
- TV layout activates on coarse-pointer large screens and supports D-pad navigation.
- Odysseus touchpoints (SendToOdysseus component, /prefix in command bar) are visible and functional when integration is configured.
- UI mode toggle (Tower ↔ Void) works and persists across page loads.

====================================================================
TECH STACK
====================================================================

Backend:

- Rust.
- Tokio async runtime.
- Axum preferred, Actix Web acceptable.
- REST API.
- WebSocket or Server-Sent Events for real-time updates.
- SQLite for local persistent state.
- TOML/YAML config files.
- Structured logging.
- OpenAPI generation.

Frontend:

- TypeScript.
- Vite.
- React, Svelte, or Solid.
- xterm.js for browser terminal.
- Modern component structure.
- No unnecessary SaaS dependencies.
- No telemetry libraries.

Storage paths:

- Config: /etc/voidtower/
- Data: /var/lib/voidtower/
- Runtime: /var/run/voidtower/
- Logs: journald by default, optional /var/log/voidtower/

Default ports:

- Main UI/API: 8743
- Agent/internal API: 8744
- Public status pages: 8745

All ports must be configurable.

====================================================================
REPOSITORY STRUCTURE
====================================================================

Create this repository structure:

voidtower/
├── backend/
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── config/
│       ├── auth/
│       ├── api/
│       ├── monitoring/
│       ├── containers/
│       ├── vms/
│       ├── services/
│       ├── storage/
│       ├── networking/
│       ├── backups/
│       ├── alerts/
│       ├── automation/
│       ├── cluster/
│       ├── terminal/
│       ├── audit/
│       └── security/
├── frontend/
│   ├── package.json
│   └── src/
│       ├── app/
│       ├── components/
│       ├── routes/
│       ├── styles/
│       └── theme/
├── app-vault/
│   └── apps/
├── packaging/
│   ├── systemd/
│   ├── deb/
│   ├── rpm/
│   ├── arch/
│   ├── alpine/
│   └── generic/
├── scripts/
│   ├── install.sh
│   ├── uninstall.sh
│   ├── build-release.sh
│   └── doctor.sh
├── docs/
├── tests/
├── .github/workflows/
├── LICENSE
├── NOTICE
├── README.md
├── SECURITY.md
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
└── docker-compose.yml

====================================================================
BACKEND REQUIREMENTS
====================================================================

The backend daemon must:

- Start as a Linux service.
- Serve the frontend.
- Expose REST API under /api.
- Expose real-time metrics via WebSocket or SSE.
- Support HTTPS where configured.
- Support HTTP for local/dev installs.
- Generate a node ID on first boot.
- Generate a bootstrap token on first install.
- Support single-node mode by default.
- Support agent mode for additional nodes.
- Store secrets securely.
- Log all sensitive actions to the audit log.

CLI flags:

- --bind
- --port
- --agent
- --config
- --no-tls
- --show-token
- --reset-node
- --leave-cluster
- --rotate-cluster-secret
- --version
- --doctor

API endpoint groups:

- /api/auth/*
- /api/nodes/*
- /api/metrics/*
- /api/containers/*
- /api/vms/*
- /api/storage/*
- /api/network/*
- /api/services/*
- /api/apps/*
- /api/backups/*
- /api/status-pages/*
- /api/alerts/*
- /api/automation/*
- /api/settings/*
- /api/audit/*
- /api/security/*
- /api/terminal/*

Include /api/openapi.json.

====================================================================
AUTHENTICATION AND SECURITY
====================================================================

Implement:

- Bootstrap token for first login.
- Local internal users.
- Optional Linux PAM authentication where available.
- Role-based access control:
  - Owner
  - Admin
  - Operator
  - Viewer
- Secure session cookies.
- Login rate limiting.
- CSRF protection.
- Audit log.
- Optional TOTP.
- Optional passkey/WebAuthn support if time permits.

Security rules:

- No default shared cluster secret.
- Generate per-installation secrets.
- Secrets must be file mode 0600.
- /etc/voidtower must be 0700.
- Never concatenate untrusted input into shell commands.
- Use structured command invocation.
- Dangerous actions require confirmation.
- Destructive actions must be logged.
- Prefer a privileged helper model.
- If the daemon must run as root for MVP, document the risks and isolate command execution.

====================================================================
MONITORING
====================================================================

Collect and expose:

- Hostname.
- Uptime.
- CPU usage.
- CPU count.
- CPU model.
- RAM total/used.
- Swap total/used.
- Disk usage.
- Filesystem usage.
- Network interfaces.
- RX/TX bytes.
- Load average.
- Process count.
- OS name/version.
- Kernel version.
- Top CPU processes.
- Top memory processes.

Provide real-time dashboard updates.

Health checks:

- High CPU.
- High RAM.
- High disk usage.
- Service failure.
- Certificate expiry.
- Backup failure.
- Network interface down.
- Docker unavailable.
- VM backend unavailable.

====================================================================
SERVICES
====================================================================

Implement systemd management:

- List services.
- View service status.
- Start service.
- Stop service.
- Restart service.
- Enable service.
- Disable service.
- View recent logs.

If systemd is unavailable, show a clear unsupported message and continue running.

====================================================================
CONTAINERS
====================================================================

Docker support:

- Connect to local Docker socket server-side.
- List containers.
- Show stats.
- Show logs.
- Start/stop/restart containers.
- Remove containers.
- Exec into containers through terminal.
- List images.
- Pull images.
- List networks.
- List volumes.
- Deploy Docker Compose projects.

LXC support where installed:

- Detect LXC.
- List containers.
- Start/stop/restart.
- Show config.
- Open console where possible.

Never expose Docker socket directly to the frontend.

====================================================================
VIRTUAL MACHINES
====================================================================

KVM/libvirt support:

- Detect libvirt.
- List VMs.
- Start/stop/reboot VMs.
- Create basic VM.
- Delete VM with confirmation.
- Show VM CPU/RAM/disk/network config.
- Support VNC/SPICE proxy if feasible.
- Support ISO selection/upload.
- Support snapshots if backend supports it.

Proxmox:

- Detect if running on Proxmox VE.
- Avoid breaking Proxmox packages.
- Show Proxmox detection state.
- Optional API integration if credentials are configured.

====================================================================
APP VAULT
====================================================================

Create an open YAML-based app catalog called App Vault.

Each app YAML must include:

- name
- slug
- description
- category
- icon
- compose template
- ports
- volumes
- environment variables
- health check
- backup paths
- update strategy

Include example apps:

- Gitea
- Jellyfin
- Home Assistant
- Vaultwarden
- Uptime Kuma
- Grafana
- Prometheus
- PostgreSQL
- MariaDB
- Redis
- Nextcloud
- Syncthing
- Pi-hole
- AdGuard Home
- Nginx Proxy Manager
- MinIO
- Code Server
- Node-RED
- Ollama
- Mealie

App deployment must generate transparent Docker Compose files under /var/lib/voidtower/apps/.

Users must be able to inspect generated Compose files before deploy.

====================================================================
TERMINAL
====================================================================

Implement browser terminal:

- xterm.js frontend.
- Backend PTY bridge.
- WebSocket transport.
- Auth required.
- Audit session start/stop.
- Optional session recording.
- Support host shell.
- Support container exec shell.

====================================================================
STORAGE AND BACKUPS
====================================================================

Storage page:

- Disks.
- Partitions.
- Mount points.
- Filesystems.
- Usage.
- Available space.

Mount helpers:

- NFS.
- SMB/CIFS.
- SSHFS.
- S3-compatible storage if feasible.

Backups:

- Prefer Restic integration.
- Support local path target.
- Support S3-compatible target.
- Support scheduled backups.
- Manual backup run.
- Restore.
- Retention policy.
- Backup logs.
- Dry-run mode.

====================================================================
NETWORKING
====================================================================

Networking page:

- Interfaces.
- IP addresses.
- Routes.
- DNS config.
- Listening ports.
- Firewall status.

Firewall support:

- UFW where available.
- firewalld where available.
- nftables where available.
- Generic read-only fallback.

Include safe-mode rollback for firewall changes that could lock the user out.

Optional:

- WireGuard peer management.
- Reverse proxy manager.
- TLS certificate manager.
- Let’s Encrypt DNS challenge support.

====================================================================
STATUS PAGES AND ALERTS
====================================================================

Status checks:

- HTTP.
- TCP.
- ICMP/ping.
- Container health.
- Service health.

Status pages:

- Public status endpoint.
- Custom title.
- Custom theme.
- Incident notes.
- Uptime history.

Alerts:

- Email SMTP.
- Discord webhook.
- Slack webhook.
- Telegram bot.
- Generic webhook.

Route alerts by severity and category.

====================================================================
AUTOMATION
====================================================================

Implement YAML-driven automation for MVP.

Triggers:

- Cron.
- Webhook.
- Alert event.
- Manual run.

Actions:

- Run command.
- HTTP request.
- Restart service.
- Start/stop container.
- Send notification.
- Run backup.
- Delay.
- Conditional branch.

Every automation run must be logged.


====================================================================
ODYSSEUS AI AGENT INTEGRATION
====================================================================

VoidTower must be fully integratable with Odysseus:

https://github.com/pewdiepie-archdaemon/odysseus

Odysseus is a self-hosted AI workspace with local-first/privacy-first goals. It supports chat with local/API models, agent workflows with tools, MCP, web, files, shell, skills, and memory, plus scheduled tasks and agent-aware notes/calendar features. VoidTower must therefore expose a safe, documented automation and infrastructure-control surface that Odysseus agents can use.

Integration goal:

Odysseus should be able to act as the AI automation brain, while VoidTower acts as the infrastructure control plane.

VoidTower must provide:

1. Odysseus-compatible API access
   - Create scoped API tokens specifically for Odysseus.
   - Tokens must support least-privilege scopes.
   - Example scopes:
     - metrics:read
     - nodes:read
     - services:read
     - services:restart
     - containers:read
     - containers:restart
     - apps:deploy
     - backups:run
     - alerts:read
     - automation:run
     - terminal:restricted
   - API tokens must be revocable.
   - Token use must be audit logged.
   - Token permissions must be visible in the UI.

2. MCP server support
   - VoidTower must include an optional built-in MCP server.
   - The MCP server should expose safe infrastructure tools for AI agents.
   - MCP server must be disabled by default.
   - Enabling MCP requires Owner/Admin approval.
   - MCP must support local-only bind by default.
   - MCP must support token authentication.
   - MCP must expose tool schemas for:
     - list_nodes
     - get_node_metrics
     - list_services
     - restart_service
     - list_containers
     - restart_container
     - get_container_logs
     - run_backup
     - get_backup_status
     - list_alerts
     - acknowledge_alert
     - run_automation
     - get_status_page_state
   - Dangerous MCP tools must require explicit allowlisting.

3. Odysseus tool manifest
   - Generate an Odysseus-compatible tool manifest/documentation file.
   - Place it at:
     - /etc/voidtower/odysseus-tools.json
     - and expose read-only via /api/integrations/odysseus/manifest
   - The manifest must describe:
     - tool name
     - description
     - input schema
     - output schema
     - required permission scope
     - whether confirmation is required
     - whether the action is destructive
   - Include example Odysseus configuration snippets in docs.

4. Webhook bridge
   - VoidTower must support incoming webhooks from Odysseus.
   - Webhooks can trigger VoidTower automations.
   - Webhook payloads must be validated.
   - Webhooks must support shared-secret signing.
   - Webhook calls must be audit logged.
   - Webhooks must support dry-run mode.

5. Automation handoff
   - VoidTower automations must be callable by Odysseus.
   - Odysseus should be able to:
     - run an automation
     - pass variables to it
     - check run status
     - read logs
     - cancel a running automation where safe
   - VoidTower must return structured results that an AI agent can understand.

6. AI-safe action model
   - Any AI-triggerable action must be classified:
     - read-only
     - low-risk
     - medium-risk
     - high-risk
     - destructive
   - High-risk and destructive actions must require one of:
     - manual confirmation in VoidTower UI
     - pre-approved policy
     - time-limited approval token
   - Examples requiring confirmation:
     - delete container
     - delete VM
     - purge backups
     - modify firewall rules
     - run arbitrary shell command
     - expose service publicly
     - rotate secrets
     - uninstall packages
   - AI agents must never receive unrestricted root shell by default.

7. Event stream for agents
   - Provide an event stream Odysseus can subscribe to.
   - Events:
     - node_down
     - high_cpu
     - high_memory
     - disk_nearly_full
     - service_failed
     - container_unhealthy
     - backup_failed
     - certificate_expiring
     - suspicious_login
     - automation_completed
   - Event stream options:
     - SSE endpoint
     - webhook outbound
     - optional MCP resource/event support

8. Odysseus integration UI
   - Add Settings → Integrations → Odysseus.
   - UI must allow:
     - enable/disable integration
     - generate API token
     - select scopes
     - enable/disable MCP server
     - configure allowed Odysseus base URL
     - configure webhook secret
     - view recent Odysseus-triggered actions
     - revoke access
   - Show copy-paste setup instructions for Odysseus.

9. Documentation
   - Add docs/integrations/odysseus.md.
   - Include:
     - what Odysseus is
     - recommended deployment topology
     - how to create a VoidTower token
     - how to add VoidTower tools to Odysseus
     - MCP setup
     - webhook setup
     - example prompts
     - safe automation examples
     - dangerous-action guardrails
   - Include example Odysseus prompts:
     - “Check my infrastructure and summarize anything unhealthy.”
     - “Restart failed non-critical services, but ask before touching databases.”
     - “If disk usage is above 90%, identify the biggest logs and draft a cleanup plan.”
     - “Run the nightly backup now and report the result.”
     - “Deploy Gitea from App Vault on node alpha with these variables.”
     - “Investigate why container X is unhealthy and suggest a fix.”

10. Security requirements
   - Odysseus integration must be off by default.
   - All AI-triggered actions must be audit logged.
   - All generated tokens must be hashed at rest.
   - Plaintext tokens shown only once.
   - Integration must work over localhost, LAN, VPN, or reverse proxy.
   - Never require exposing VoidTower or Odysseus directly to the public internet.
   - Provide rate limiting for Odysseus API tokens.
   - Provide emergency “Disable all AI access” button.

11. Optional future enhancement
   - Add a “Send to Odysseus” button on alerts, services, containers, VMs, backups, and logs.
   - This should package the selected context and open/create an Odysseus investigation task.
   - Include context safely:
     - metrics
     - recent logs
     - service/container state
     - node metadata
   - Redact secrets automatically.
   
   
====================================================================
MULTI-NODE AGENT MODE
====================================================================

VoidTower must work as:

1. Main node:
   - Serves UI.
   - Stores cluster state.
   - Manages other nodes.

2. Agent node:
   - Runs same binary with --agent.
   - Does not serve full UI.
   - Exposes authenticated agent API.
   - Reports metrics.
   - Accepts limited commands from main node.

Cluster features:

- Add node with join token.
- Remove node.
- Poll metrics.
- Show node health.
- Proxy selected actions.
- Rotate cluster secret.
- Leave cluster.
- Reset node.

Do not require etcd, Consul, Kubernetes, or external consensus for MVP.

====================================================================
ONE-SHOT INSTALLER REQUIREMENT
====================================================================

Create a one-shot installer script:

scripts/install.sh

The installer must allow this style of installation:

curl -fsSL https://example.invalid/voidtower/install.sh | sudo bash

The installer must also support:

curl -fsSL https://example.invalid/voidtower/install.sh | sudo bash -s -- --yes
curl -fsSL https://example.invalid/voidtower/install.sh | sudo bash -s -- --agent
curl -fsSL https://example.invalid/voidtower/install.sh | sudo bash -s -- --port 8743
curl -fsSL https://example.invalid/voidtower/install.sh | sudo bash -s -- --install-dir /opt/voidtower
curl -fsSL https://example.invalid/voidtower/install.sh | sudo bash -s -- --from-source

The installer must be as distribution-compatible as realistically possible across Linux.

Support these package managers:

- apt
- apt-get
- dnf
- yum
- zypper
- pacman
- apk
- xbps-install
- emerge
- eopkg
- swupd
- nix-env if available
- generic fallback mode

Support these distributions/families:

- Debian
- Ubuntu
- Linux Mint
- Pop!_OS
- Fedora
- RHEL
- CentOS
- Rocky Linux
- AlmaLinux
- openSUSE
- SLES
- Arch Linux
- Manjaro
- EndeavourOS
- Alpine Linux
- Void Linux
- Gentoo
- Solus
- Clear Linux
- NixOS where possible
- Proxmox VE
- Raspberry Pi OS
- DietPi
- Armbian
- Generic systemd Linux
- Generic non-systemd Linux with degraded/manual service instructions

Installer behavior:

- Must require root or sudo.
- Must detect OS from /etc/os-release.
- Must detect architecture using uname -m.
- Must support x86_64 and aarch64 prebuilt binaries.
- Must fall back to source build if no binary exists.
- Must detect systemd.
- Must create systemd service when systemd exists.
- Must provide OpenRC/runit/manual fallback instructions where relevant.
- Must not perform full system upgrades.
- Must refresh package indexes only when needed.
- Must install minimum required dependencies.
- Must warn before installing heavy optional dependencies.
- Must detect existing install and upgrade safely.
- Must preserve config on upgrade.
- Must generate bootstrap token on first install.
- Must create /etc/voidtower with 0700 permissions.
- Must create /var/lib/voidtower.
- Must create /var/log/voidtower if file logging enabled.
- Must install binary to /usr/local/bin/voidtower by default.
- Must install frontend assets.
- Must install systemd unit.
- Must enable and start service.
- Must print final URL, bootstrap token location, service status, and log commands.
- Must create uninstall script at /usr/local/bin/voidtower-uninstall.

Installer preflight checks:

- Existing /etc/voidtower.
- Existing /var/lib/voidtower.
- Port conflicts for 8743, 8744, 8745.
- Firewall active without ports open.
- Docker availability.
- LXC availability.
- KVM/libvirt availability.
- systemd availability.
- Disk space.
- CPU architecture.
- SELinux/AppArmor presence.
- Proxmox detection.
- Running as root.
- curl/wget availability.
- tar/unzip availability.

Installer must avoid breaking the host:

- Never remove packages.
- Never disable existing services unless explicitly required and confirmed.
- Never change firewall rules automatically unless user passes explicit flag.
- Never overwrite existing config without backup.
- Create timestamped config backups during upgrade.
- Print warnings for conflicts.
- Continue with degraded mode where possible.

Installer flags:

- --yes
- --agent
- --port <port>
- --bind <address>
- --install-dir <path>
- --data-dir <path>
- --config-dir <path>
- --no-start
- --no-enable
- --no-systemd
- --from-source
- --version <version>
- --channel stable|nightly
- --uninstall
- --dry-run
- --help

Uninstaller:

scripts/uninstall.sh

Must:

- Stop service.
- Disable service.
- Remove binary.
- Remove systemd unit.
- Ask before deleting config/data.
- Support --purge to remove all data.
- Preserve backups unless --purge is explicitly passed.

====================================================================
PACKAGING
====================================================================

Provide:

- systemd unit.
- Dockerfile.
- docker-compose.yml.
- Debian packaging skeleton.
- RPM packaging skeleton.
- Arch PKGBUILD.
- Alpine package notes.
- Generic tarball install.

Release artifacts should include:

- voidtower-x86_64-linux
- voidtower-aarch64-linux
- frontend assets
- install.sh
- uninstall.sh
- SHA256SUMS
- SBOM

====================================================================
CI/CD
====================================================================

Create GitHub Actions workflows for:

- Rust fmt.
- Rust clippy.
- Rust tests.
- Frontend lint.
- Frontend build.
- Backend build.
- Security audit if possible.
- SBOM generation.
- Release artifact build.
- SHA256 checksums.

====================================================================
DOCUMENTATION
====================================================================

README must include:

- What VoidTower is.
- Screenshots placeholders.
- One-command install.
- Manual install.
- Docker install.
- Upgrade.
- Uninstall.
- Security model.
- Default ports.
- Config paths.
- Troubleshooting.
- Development setup.
- License.

README intro:

# VoidTower

VoidTower is a self-hosted infrastructure command tower for Linux servers.

Monitor hosts, control containers, manage virtual machines, inspect services,
open terminals, deploy apps, run backups, publish status pages, and automate
operational tasks from one local-first web interface.

No telemetry. No cloud lock-in. No license server. No nonsense.

====================================================================
MVP PRIORITY
====================================================================

Build in this order:

Phase 1:
- Repository scaffold.
- Backend daemon.
- Config loading.
- SQLite database.
- Bootstrap auth.
- Session auth.
- Dark VoidTower frontend shell.
- Dashboard.
- System metrics.
- systemd service list/status/restart.
- Web terminal.
- Audit log.
- One-shot installer.
- README.

Phase 2:
- Docker container listing/actions.
- Docker logs.
- App Vault Docker Compose deployment.
- Alerts.
- Status checks.
- Restic backup integration.

Phase 3:
- LXC support.
- KVM/libvirt support.
- Agent mode.
- Multi-node dashboard.
- Firewall safe-mode rollback.

Phase 4:
- Automation engine.
- WireGuard manager.
- Reverse proxy manager.
- OIDC/passkeys.
- Plugin SDK.

====================================================================
ACCEPTANCE CRITERIA
====================================================================

The generated repository is acceptable only if:

- It builds successfully.
- It has a real backend.
- It has a real frontend.
- It has a real installer.
- A fresh Linux VM can install it with one command.
- Web UI is available on port 8743 by default.
- First login works with bootstrap token.
- CPU/RAM/disk/network metrics update.
- systemd services can be viewed.
- A browser terminal can be opened.
- Docker containers are listed if Docker is installed.
- App Vault contains example apps.
- Installer supports major Linux distributions and has generic fallback behavior.
- No external SaaS dependency exists.
- No telemetry exists.
- Source code is AGPL-3.0-or-later.
- No copied WolfStack branding, assets, text, names, or code exist.
- VoidTower includes an Odysseus integration page.
- VoidTower can generate scoped API tokens for Odysseus.
- VoidTower exposes an Odysseus tool manifest.
- VoidTower includes optional MCP server support for AI-agent tools.
- Odysseus can call VoidTower automations through API or webhook.
- AI-triggered actions are permission-scoped, risk-classified, and audit logged.
- High-risk/destructive AI actions require confirmation or explicit pre-approved policy.
- Documentation includes a complete Odysseus integration guide.
- VoidTower includes a polished, responsive, dark cyber-ops web UI.
- VoidTower includes a command palette and keyboard shortcuts.
- VoidTower includes a full Theme Editor under Settings → Appearance.
- Users can customize colors, density, typography, glow, animations, terminal colors, and layout behavior.
- Themes can be saved, previewed, applied live, duplicated, imported, exported, and reset.
- Theme customization is implemented safely with validated CSS variables and no arbitrary CSS injection.
- The UI includes clear loading, empty, error, offline, warning, and danger states.
- Mobile and tablet layouts are usable for common admin actions.

Now implement the complete repository.
