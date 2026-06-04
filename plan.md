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
- Mobile browsers.
- PWA/mobile install mode.

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
OVERALL UI CONCEPT
--------------------------------------------------------------------

VoidTower should feel like logging into a tactical server operations tower.

Visual metaphors:

- Command tower.
- Dark terminal.
- Infrastructure grid.
- Machine telemetry.
- Node map.
- Signal beacon.
- Control console.
- Systems cockpit.

The UI should combine:

- A left navigation sidebar.
- A top command/search bar.
- A main content canvas.
- Optional right-side context drawer.
- Optional bottom task/activity console.
- Modal confirmations for dangerous actions.
- Toasts for lightweight feedback.
- Persistent activity/audit feed for important operations.

Default layout:

1. Left sidebar
   - Product mark.
   - Current node/cluster selector.
   - Primary navigation.
   - Health badges.
   - User/account controls.
   - Collapse/expand state.

2. Top bar
   - Page title.
   - Global search.
   - Command palette trigger.
   - Quick actions.
   - Alert indicator.
   - Theme/status controls.
   - Current user menu.

3. Main panel
   - Page-specific content.
   - Cards, tables, charts, terminal panels, forms, and split views.

4. Right context drawer
   - Details for selected node/container/service/VM.
   - Recent logs.
   - Related actions.
   - AI/Odysseus handoff button.
   - Audit trail for selected entity.

5. Bottom task console
   - Background jobs.
   - Automation runs.
   - Deployment progress.
   - Backup progress.
   - Agent-triggered tasks.
   - Expandable log output.

--------------------------------------------------------------------
NAVIGATION STRUCTURE
--------------------------------------------------------------------

Primary navigation sections:

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

Navigation requirements:

- Sidebar must be collapsible.
- Sidebar must show health badges where useful.
- Sections with active warnings must show alert indicators.
- Navigation must support keyboard shortcuts.
- Current section must be visually obvious.
- Mobile navigation must become a slide-out drawer.
- The UI must include breadcrumbs for deeper views.

Keyboard shortcuts:

- `/` opens global search.
- `Ctrl+K` or `Cmd+K` opens command palette.
- `g d` opens Command/Dashboard.
- `g n` opens Nodes.
- `g c` opens Containers.
- `g v` opens Virtual Machines.
- `g a` opens App Vault.
- `g t` opens Terminal.
- `g s` opens Settings.
- `Esc` closes modals/drawers.
- `?` opens keyboard shortcut help.

--------------------------------------------------------------------
COMMAND PALETTE
--------------------------------------------------------------------

VoidTower must include a command palette.

The command palette should allow users to:

- Navigate to pages.
- Search nodes.
- Search containers.
- Search VMs.
- Search services.
- Run safe actions.
- Open logs.
- Start terminal sessions.
- Trigger automations.
- Open settings sections.
- Search App Vault.
- Send selected context to Odysseus if enabled.

Command palette actions must respect RBAC permissions.

Dangerous actions must not execute directly from the command palette unless they require confirmation.

Examples:

- “Restart nginx”
- “Open terminal on node alpha”
- “Show failed services”
- “Run backup: nightly”
- “Deploy Gitea”
- “View Docker logs for jellyfin”
- “Open security scanner”
- “Send alert to Odysseus”

--------------------------------------------------------------------
DASHBOARD / COMMAND PAGE
--------------------------------------------------------------------

The main landing page is called Command.

It must show:

- Cluster health.
- Node count.
- Online/offline nodes.
- CPU/RAM/disk overview.
- Network throughput.
- Active alerts.
- Failed services.
- Unhealthy containers.
- VM status.
- Backup status.
- Recent automation runs.
- Recent audit events.
- Security warnings.
- Quick actions.

Dashboard cards should be compact and useful.

Each card should support:

- Loading state.
- Empty state.
- Healthy state.
- Warning state.
- Critical state.
- Click-through to detail page.

Charts:

- CPU history.
- RAM history.
- Disk usage.
- Network RX/TX.
- Node health over time.
- Backup success/failure trend.

Charts must be readable in dark mode and not overuse neon glow.

--------------------------------------------------------------------
TABLES AND DATA GRIDS
--------------------------------------------------------------------

VoidTower will display many operational tables. Tables must be excellent.

Requirements:

- Sortable columns.
- Filter/search.
- Status badges.
- Bulk selection where safe.
- Row actions.
- Expandable row details.
- Column visibility controls.
- Compact and comfortable density modes.
- Pagination or virtual scrolling for large datasets.
- Copy-to-clipboard for IDs, IPs, paths, commands, tokens where appropriate.
- Clear empty states.
- Clear loading states.
- Clear error states.

Tables needed for:

- Nodes.
- Containers.
- Images.
- Volumes.
- Networks.
- VMs.
- Services.
- Backups.
- Alerts.
- Automations.
- Audit logs.
- API tokens.
- Odysseus integration events.

--------------------------------------------------------------------
DETAIL PAGES
--------------------------------------------------------------------

Every major object should have a detail page:

- Node detail.
- Container detail.
- VM detail.
- Service detail.
- App detail.
- Backup detail.
- Alert detail.
- Automation detail.

Detail pages should include:

- Summary header.
- Health/status badge.
- Metadata.
- Metrics.
- Logs.
- Related resources.
- Recent events.
- Audit history.
- Safe action buttons.
- Dangerous action area separated visually.

Dangerous actions must be grouped under a clearly marked “Danger Zone”.

--------------------------------------------------------------------
TERMINAL UI
--------------------------------------------------------------------

Terminal is a central part of VoidTower.

Requirements:

- xterm.js-based terminal.
- Full-screen mode.
- Split terminal panes if feasible.
- Node selector.
- Container exec selector.
- Font size controls.
- Copy/paste support.
- Session status indicator.
- Reconnect handling.
- Clear disconnect state.
- Optional session recording indicator.
- Audit notice when session recording is enabled.

Terminal design:

- Background: near-black.
- Text: terminal green by default.
- Cursor: bright accent.
- Minimal chrome.
- Clear session metadata:
  - node
  - user
  - shell
  - started time
  - recording state

--------------------------------------------------------------------
LOG VIEWER
--------------------------------------------------------------------

Logs must be easy to inspect.

Requirements:

- Live tail mode.
- Pause/resume.
- Search within logs.
- Regex filter if feasible.
- Severity highlighting.
- Timestamp normalization.
- Download logs.
- Copy selected lines.
- Wrap/nowrap toggle.
- Follow mode.
- Jump to bottom.
- Redact secrets where possible.

Log viewer should be used for:

- systemd services.
- containers.
- VMs where available.
- backups.
- automations.
- installer logs.
- audit events.
- Odysseus-triggered actions.

--------------------------------------------------------------------
APP VAULT UI
--------------------------------------------------------------------

App Vault is the application deployment area.

Design:

- Dark app catalog.
- Cards for apps.
- Category filters.
- Search.
- Badges for official/community/local templates.
- Deployment wizard.
- Compose preview.
- Environment variable editor.
- Volume/port editor.
- Validation before deploy.
- Deployment progress.
- Rollback info where available.

Deployment wizard steps:

1. Select app.
2. Choose target node.
3. Configure variables.
4. Configure ports/volumes.
5. Preview generated Docker Compose.
6. Confirm deployment.
7. Watch deployment logs.

--------------------------------------------------------------------
AUTOMATION UI
--------------------------------------------------------------------

Automation must feel powerful but safe.

Views:

- Automation list.
- Automation editor.
- Run history.
- Trigger configuration.
- Action graph/YAML editor.
- Logs.
- Manual run panel.
- Odysseus handoff/invocation history.

MVP editor:

- YAML editor with validation.
- Schema-aware suggestions if possible.
- Dry-run button.
- Save draft.
- Enable/disable automation.

Future editor:

- Visual workflow graph.
- Drag-and-drop triggers/actions.
- Conditional branches.
- Secrets picker.
- Run preview.

--------------------------------------------------------------------
ALERTS UI
--------------------------------------------------------------------

Alerts must be impossible to miss but not obnoxious.

Alert states:

- Info.
- Warning.
- Critical.
- Resolved.
- Acknowledged.
- Silenced.

Alert page must support:

- Filtering by severity.
- Filtering by node.
- Filtering by category.
- Acknowledge.
- Silence.
- Resolve.
- Assign owner if users exist.
- Send to Odysseus.
- View related logs.
- View related metrics.
- View timeline.

Critical alerts should appear in:

- Sidebar badge.
- Top bar indicator.
- Dashboard card.
- Alerts page.

--------------------------------------------------------------------
SECURITY UI
--------------------------------------------------------------------

Security section must be blunt and useful.

Views:

- Security overview.
- File permission scanner.
- Exposed services.
- Login attempts.
- Active sessions.
- API tokens.
- Odysseus integration access.
- Audit log.
- Secret rotation.
- TLS/certificate status.
- Dangerous capability review.

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

--------------------------------------------------------------------
SETTINGS UI
--------------------------------------------------------------------

Settings must be comprehensive but organized.

Settings sections:

- General
- Appearance
- Theme Editor
- Users
- Roles & Permissions
- Authentication
- Sessions
- API Tokens
- Odysseus Integration
- MCP Server
- Notifications
- Alerts
- Backups
- Network
- TLS
- App Vault
- Automation
- Security
- Advanced
- About

Settings must include search.

Settings changes must show:

- Unsaved state.
- Validation errors.
- Reset/revert option.
- Save confirmation for sensitive changes.
- Audit logging for sensitive settings.

--------------------------------------------------------------------
FULL THEME CUSTOMIZATION
--------------------------------------------------------------------

VoidTower must allow the user to fully customize the UI theme from inside the web UI.

Add Settings → Appearance → Theme Editor.

The Theme Editor must allow customization of:

- Base mode:
  - dark
  - darker
  - light
  - custom
- Background colors.
- Panel colors.
- Card colors.
- Border colors.
- Text colors.
- Muted text colors.
- Primary accent color.
- Secondary accent color.
- Success color.
- Warning color.
- Danger color.
- Terminal background.
- Terminal foreground.
- Terminal cursor color.
- Chart colors.
- Sidebar style.
- Font family.
- Font size scale.
- UI density:
  - compact
  - normal
  - comfortable
- Border radius:
  - sharp
  - slight
  - rounded
- Glow intensity:
  - off
  - low
  - medium
  - high
- Animation level:
  - off
  - reduced
  - normal
- Table density.
- Terminal font.
- Code/log font.
- Card shadow depth.
- Sidebar width.
- Layout mode:
  - fixed
  - fluid
  - dense ops

Theme requirements:

- Themes must be stored locally.
- Themes must be exportable as JSON.
- Themes must be importable from JSON.
- Users must be able to duplicate a theme.
- Users must be able to reset to defaults.
- Users must be able to preview before applying.
- Users must be able to save multiple named themes.
- Admins can set a global default theme.
- Users can override the global theme for their own account.
- Theme changes should apply live without reload.
- Invalid colors must be rejected.
- Accessible contrast warnings must be shown.
- Theme editor must never allow unsafe CSS injection.

Built-in themes:

1. VoidTower Default
   - Dark cyber-ops theme.
   - Violet/cyan accents.

2. Blacksite
   - Near-black.
   - Red danger accents.
   - Minimal glow.

3. Ghost Terminal
   - Black/green terminal-inspired theme.

4. Deep Grid
   - Indigo/cyan infrastructure-grid theme.

5. Solar Breach
   - Dark amber/orange operations theme.

6. Light Ops
   - Light theme for daylight environments.

7. High Contrast
   - Accessibility-first high-contrast theme.

Theme implementation:

- Use CSS variables.
- Store theme tokens in database or config.
- Apply user theme at login.
- Expose current theme at `/api/settings/theme`.
- Provide endpoints:
  - GET /api/settings/themes
  - POST /api/settings/themes
  - PUT /api/settings/themes/{id}
  - DELETE /api/settings/themes/{id}
  - POST /api/settings/themes/{id}/apply
  - POST /api/settings/themes/import
  - GET /api/settings/themes/{id}/export

Theme JSON schema must be documented.

Example theme JSON:

{
  "name": "Ghost Terminal",
  "mode": "dark",
  "tokens": {
    "bgRoot": "#020403",
    "bgPanel": "#050806",
    "bgCard": "#08110c",
    "textPrimary": "#d7ffe8",
    "textSecondary": "#7cffb2",
    "accentPrimary": "#00ff9c",
    "accentSecondary": "#00b8ff",
    "accentDanger": "#ff3355",
    "borderSubtle": "#123022",
    "terminalBg": "#000000",
    "terminalFg": "#00ff9c",
    "terminalCursor": "#ffffff"
  },
  "density": "compact",
  "radius": "slight",
  "glow": "medium",
  "animations": "reduced"
}

--------------------------------------------------------------------
ACCESSIBILITY
--------------------------------------------------------------------

VoidTower must be accessible enough for serious daily use.

Requirements:

- Keyboard navigable.
- Visible focus states.
- Semantic HTML.
- ARIA labels where needed.
- Sufficient color contrast.
- High contrast theme.
- Reduced motion support.
- Screen-reader-friendly status updates where practical.
- Do not rely on color alone for state.
- Icons must have labels/tooltips.

Theme editor must warn if custom colors create poor contrast.

--------------------------------------------------------------------
RESPONSIVE / MOBILE UI
--------------------------------------------------------------------

Mobile UI must not be an afterthought.

Requirements:

- Sidebar becomes slide-out drawer.
- Tables become cards or horizontally scrollable with sticky key columns.
- Terminal usable on mobile.
- Important actions reachable with touch.
- Cards stack cleanly.
- Charts remain readable.
- Command palette works on mobile.
- Status/alerts are visible.
- PWA metadata included.

Mobile use cases:

- Check alerts.
- Restart a service.
- View logs.
- Run an automation.
- Check backup result.
- Open limited terminal if allowed.
- Approve or deny a high-risk Odysseus action.

--------------------------------------------------------------------
REAL-TIME UX
--------------------------------------------------------------------

The UI must clearly show live state.

Requirements:

- Real-time metric updates.
- Connection status indicator.
- Reconnecting state.
- Stale data indicator.
- Last updated timestamp.
- Background job progress.
- Toasts for completed actions.
- Persistent task log for long operations.
- Optimistic updates only when safe.

If the backend connection drops:

- Show degraded/offline banner.
- Avoid pretending actions succeeded.
- Queue nothing silently.
- Disable unsafe actions until reconnected.

--------------------------------------------------------------------
CONFIRMATIONS AND DANGER ZONES
--------------------------------------------------------------------

Dangerous actions must use strong confirmation UX.

Actions requiring confirmation:

- Delete container.
- Delete VM.
- Delete volume.
- Delete backup.
- Purge data.
- Modify firewall rules.
- Expose service publicly.
- Rotate secrets.
- Run arbitrary command.
- Disable authentication.
- Enable MCP/Odysseus high-risk tools.
- Uninstall package.
- Remove node.
- Reset cluster.

Confirmation dialog must show:

- Exact target.
- Consequence.
- Whether action is reversible.
- Required permission.
- Audit logging note.
- Optional typed confirmation for destructive actions.

Example:

“To delete VM `prod-db-01`, type `prod-db-01`.”

--------------------------------------------------------------------
ODYSSEUS UI TOUCHPOINTS
--------------------------------------------------------------------

If Odysseus integration is enabled, the UI must include AI-agent handoff controls.

Add “Send to Odysseus” buttons on:

- Alerts.
- Failed services.
- Containers.
- VMs.
- Backup failures.
- Security findings.
- Log selections.
- Automation failures.
- Node health pages.

Send-to-Odysseus flow:

1. User selects context.
2. VoidTower previews what data will be shared.
3. Secrets are redacted.
4. User chooses:
   - ask for diagnosis
   - draft fix plan
   - run approved automation
   - monitor this issue
5. Action is logged in audit trail.

AI approval UI:

- Show pending high-risk AI-requested actions.
- User can approve once.
- User can deny.
- User can approve with time limit.
- User can create policy from repeated safe action.
- Every decision is logged.

--------------------------------------------------------------------
FRONTEND CODE QUALITY
--------------------------------------------------------------------

Frontend must be maintainable.

Requirements:

- Componentized structure.
- Reusable cards/tables/modals/forms.
- Central API client.
- Central auth state.
- Central theme store.
- Central notification/toast system.
- Type-safe API models.
- Loading/error/empty states for every data view.
- No hardcoded API URLs.
- No hardcoded colors outside theme tokens.
- No inline secrets.
- No telemetry dependencies.

Recommended component groups:

- Layout
- Navigation
- CommandPalette
- MetricCards
- Charts
- DataTable
- EntityHeader
- StatusBadge
- LogViewer
- Terminal
- ConfirmDialog
- DangerZone
- ThemeEditor
- SettingsForms
- AutomationEditor
- AppVault
- OdysseusIntegration

--------------------------------------------------------------------
WEB UI ACCEPTANCE CRITERIA
--------------------------------------------------------------------

The web UI is acceptable only if:

- It has a polished dark VoidTower identity.
- It has responsive layout.
- It has dashboard metrics.
- It has working navigation.
- It has command palette/search.
- It has usable tables.
- It has detail pages for major resources.
- It has working terminal UI.
- It has loading, empty, error, and offline states.
- It has clear dangerous-action confirmations.
- It has Settings → Appearance → Theme Editor.
- Users can fully customize the theme from the UI.
- Themes can be saved, duplicated, imported, exported, reset, and applied live.
- Theme customization uses safe CSS variables, not arbitrary unsafe CSS injection.
- Accessibility basics are implemented.
- Mobile layout is usable.
- Odysseus integration has visible UI touchpoints when enabled.

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
