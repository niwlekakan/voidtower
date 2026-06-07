VoidTower UI Design Plan
Tower Mode / Void Mode

Last updated: 2026-06-06
Status: Living document. Sections marked [BUILT] are implemented. Sections marked [PLANNED] are approved for implementation. Sections marked [FUTURE] are backlog ideas not yet scoped.

---

1. Product vision

VoidTower has two interface modes:

Tower Mode

The admin control tower. Clear, structured, dense, and predictable.

Best for:
- Settings and configuration
- Infrastructure administration
- Tables and bulk actions
- App catalog management
- Advanced debugging
- Operators who prefer a traditional dashboard

Void Mode

The living workspace. A browser-based operating system for managing apps, VMs, AI tools, terminals, logs, and infrastructure.

Best for:
- Daily operator use
- Multitasking across multiple resources at once
- AI-assisted workflows with Odysseus
- Monitoring while working
- Touch-first and tablet use
- Kiosk displays and wall panels
- TV / large-screen remote monitoring

The core idea:

Tower Mode is where you configure the system. Void Mode is where you live inside it.

---

2. Build status summary [BUILT]

The following is implemented and shipped as of this writing.

Void Mode shell:
- Floating draggable/resizable panels (AiosPanel)
- Panel snap zones: left-half, right-half, top/bottom halves, four quarters, fullscreen, minimized, sheet
- Coupled split pairs with a draggable divider (AiosSplitDivider)
- Four virtual workspaces, switched with Ctrl+1-4
- Panel cap by device tier (phone: 1, tablet: 3, desktop: 5, large: 8)
- Panels persist position, size, and workspace across session via sessionStorage
- Panel error boundary: one broken page cannot blank the whole canvas
- Send-to-workspace via title bar
- Pin-on-top toggle

Status bar (AiosStatusBar):
- CPU / RAM / NET / GPU / uptime metric pills, color-coded at warn/critical thresholds
- Workspace dot switcher
- WebSocket connection indicator
- Notification bell with dropdown
- Live clock (HH:MM, 24h, tabular)
- UI Mode Toggle pill (Tower ↔ Void, also Ctrl+Shift+V)
- Split exit button when a split pair is active

Dock (AiosDock):
- Vertical strip on ≥1400px screens
- Centered floating pill on desktop/tablet
- Full-width bottom tab bar on phone
- Open / minimized indicator dots per item
- Tooltips on hover
- 29 navigation items across all sections

Command bar (AiosCommandBar):
- ⌘K / Ctrl+K shortcut
- App name fuzzy search with icon results
- `/` prefix routes query to Odysseus via postMessage
- URL prefix opens as iframe embed panel
- Phone: FAB + bottom sheet layout

Device tier system (useDeviceTier):
- Tiers: phone / tablet / desktop / large / tv / kiosk
- Detected from window width + pointer/hover media queries
- Manual override via localStorage key `vt-device-mode`
- ?mode=kiosk URL param forces kiosk tier

TV layout (AiosTvLayout):
- 3×2 icon+label grid (D-pad navigable)
- Arrow keys to move focus, Enter to open, Escape to go back
- Fullscreen expanded view per tile

Kiosk layout (AiosKioskLayout):
- Auto-cycling tile grid (configurable interval, default 30s)
- Idle screensaver after 10 min (clock overlay, 50% dim)
- Optional 4-digit PIN to unlock interactive mode
- Interactive mode timer (5 min, then returns to passive)
- Critical alert flash: polls /api/alerts every 60s, red border flash on critical
- Config via localStorage `kiosk_layout`

Embed panels:
- Any http(s) URL in the command bar opens as an iframe panel
- Sandbox: allow-scripts, allow-same-origin, allow-forms, allow-popups

Mode toggle:
- UiModeToggle component in status bar
- Ctrl+Shift+V global shortcut
- Persisted in theme store / localStorage

Tower Mode layout (AppLayout + Sidebar + TopBar):
- Fixed left sidebar with collapsible nav groups
- Top bar with search, command palette trigger, alert badge
- Full-width main content area

All VoidTower pages are registered in both modes:
Dashboard, Services, Containers, VMs, App Vault, Storage, Files, Network, Proxies, WireGuard, Firewall, Backups, Automation, Alerts, Timeline, Terminal, AI, Models, Capabilities, Diagnostics, Secrets, Security, Audit Log, Tags, Integrations, Updates, Mods, Themes, Settings.

---

3. Mode architecture [BUILT]

```
type UIMode = "tower" | "void"
```

Stored in Zustand theme store, persisted in localStorage. Applied immediately on load without a page reload.

Toggle is:
- Visible in the Void Mode status bar (right side pill)
- Visible in the Tower Mode top bar
- Accessible via Ctrl+Shift+V from anywhere
- Touch accessible

---

4. Tower Mode design [BUILT]

Tower Mode preserves the classic admin/dashboard structure.

Characteristics:
- Dense and fast
- Table and form friendly
- Low animation
- Predictable fixed layout
- Power-user oriented

Layout:

```
┌─────────────────────────────────────────────────────────────┐
│ Top Bar: title / search / alert badge / mode toggle / user  │
├────────────────────┬────────────────────────────────────────┤
│ Sidebar            │ Main content                           │
│                    │                                        │
│ Infrastructure ▾   │ Tables, forms, cards, charts           │
│   Dashboard        │                                        │
│   Services         │                                        │
│   Containers       │                                        │
│   VMs              │                                        │
│   App Vault        │                                        │
│   Storage          │                                        │
│   Files            │                                        │
│                    │                                        │
│ Network ▾          │                                        │
│   Network          │                                        │
│   Proxies          │                                        │
│   WireGuard        │                                        │
│   Firewall         │                                        │
│                    │                                        │
│ Operations ▾       │                                        │
│   ...              │                                        │
│ System ▾           │                                        │
│   ...              │                                        │
└────────────────────┴────────────────────────────────────────┘
```

Tower Mode should not try to become a desktop. Keep it clean and direct.

---

5. Void Mode shell design [BUILT]

The shell has five layers:

1. AnimatedBackground — canvas-based animated background (7 presets, 4 glass levels)
2. AiosStatusBar — 28px top bar with metrics, workspaces, clock, alerts, mode toggle
3. Panel canvas — the floating window space between status bar and dock
4. AiosDock — icon strip (vertical left / horizontal pill / phone bottom bar)
5. AiosCommandBar — floating search/command pill above the dock

```
┌──────────────────────────────────────────────────────────────┐
│ VoidTower   CPU 12%  RAM 44%  NET ↓2M/s   ●●●○   Wifi  14:32 │  ← status bar
├──────────────────────────────────────────────────────────────┤
│                                                              │
│   ┌──────────────────────┐   ┌──────────────────────────┐   │
│   │ Dashboard            │   │ Containers               │   │  ← floating panels
│   │ ...metrics...        │   │ ...container list...     │   │
│   └──────────────────────┘   └──────────────────────────┘   │
│                                                              │
│   ┌──────────────────────────────────────────────────────┐  │
│   │ Terminal                                             │  │
│   └──────────────────────────────────────────────────────┘  │
│                                                              │
│              ┌───────────────────────────────┐              │
│              │ Open app or /ask Odysseus… ⌘K │              │  ← command bar
│              └───────────────────────────────┘              │
│  ┌──┬──┬──┬──┬──┬──┬──┬──┬──┬──┐                           │
│  │⊞ │⚡│🕒│📦│🖥│⌨│💾│🔒│⚙ │...│                           │  ← dock pill
│  └──┴──┴──┴──┴──┴──┴──┴──┴──┴──┘                           │
└──────────────────────────────────────────────────────────────┘
```

---

6. Void Mode panel system [BUILT]

See section 2 for full build status. Key design decisions:

Panel caps enforce focus. When you exceed the cap, the oldest panel is minimized rather than refusing to open. This prevents accidental accumulation on small screens.

Workspaces are lightweight — just a filter on which panels are visible. No backend state needed for MVP; sessionStorage is sufficient.

Split pairs auto-couple when two panels are snapped left-half and right-half. The divider is draggable to adjust the ratio.

Embed panels are a first-class panel type. Any URL can be embedded. This is the mechanism for bringing Gitea, Grafana, Jellyfin, and other self-hosted apps into the Void Mode canvas alongside VoidTower pages.

---

7. Workspace manager [BUILT - partial]

Current state:
- 4 workspaces (indexed 0–3), switched with Ctrl+1-4 or workspace dot clicks
- Panels are scoped to a workspace index
- Workspace state persists in sessionStorage (survives page refresh, cleared on tab close)

Planned additions [PLANNED]:
- Named workspaces (user can name them: "VM Lab", "Media", "Debugging")
- Workspace icon/emoji
- Save workspace as a named preset
- Restore a saved workspace preset
- Reset workspace (close all panels on this workspace)
- Workspace list visible in command bar or status bar tooltip

Future [FUTURE]:
- Backend persistence of workspace state (survives browser close, syncs across devices)
- Workspace templates: "VM + Odysseus", "Debug Layout", "Media", "Android Lab"
- Share workspace layout with other users on the same instance

---

8. Window manager [BUILT]

The panel system is the window manager. Current capabilities:

- Open: via dock click, command bar, or command palette
- Close: title bar X button, Ctrl+W
- Close all: Ctrl+M
- Focus: click panel, brings to front (z-index)
- Drag: title bar drag (desktop/tablet)
- Resize: edge/corner handles (desktop/tablet)
- Minimize: Escape, title bar minimize, or clicking the dock icon of an open panel
- Restore minimized: click dock icon
- Maximize to fullscreen: title bar maximize button, or Ctrl+Shift+ArrowUp
- Restore from fullscreen/snap: Ctrl+Shift+ArrowDown
- Snap left-half: drag to left edge, or Ctrl+Shift+ArrowLeft
- Snap right-half: drag to right edge, or Ctrl+Shift+ArrowRight
- Snap fullscreen: Ctrl+Shift+ArrowUp
- Quarter snaps: top-left, top-right, bottom-left, bottom-right (drag to corner)
- Alt+Tab: cycle visible panels on current workspace
- Pin: stays on top of other panels (title bar pin icon)
- Send to workspace: title bar context menu

Planned [PLANNED]:
- Right-click title bar context menu: minimize, maximize, send to workspace, pin, duplicate
- "Open beside Odysseus": shortcut to snap current panel left and open AI panel right
- Duplicate panel: open a second instance of the same page on the same workspace
- Tab groups: multiple pages within one panel (tabbed interface, groupId support already in store)

Future [FUTURE]:
- Picture-in-picture: a floating mini panel that stays visible while other panels are in focus
- Panel locking: prevent accidental move/resize of a specific panel
- Named panel presets: save the current geometry/size of a panel as a named preset

---

9. Split screen system [BUILT - partial]

Current:
- Auto-couple when two panels snap left-half + right-half
- Draggable split divider (AiosSplitDivider) with ratio 0.15–0.85
- Split pair tracked in store; uncoupling via status bar button or Ctrl+Shift+ArrowDown

Supported split layouts today:

Two-panel 50/50:
```
┌────────────────────────────┬────────────────────────────┐
│ Panel A                    │ Panel B                    │
└────────────────────────────┴────────────────────────────┘
```

Any two panels can be split this way. "VM + Odysseus" and "Logs + Metrics" are the primary use cases.

Planned [PLANNED]:
- "Open beside Odysseus" quick action: one button to snap current panel left and open AI right
- Three-panel layouts: main (center wide) + left sidebar + right sidebar (Ctrl+Shift+3?)
- Bottom drawer layout: main panel on top, smaller horizontal panel at bottom for logs/terminal
- Preset layout templates accessible from command bar

Future [FUTURE]:
- Four-panel debug grid (2×2)
- Named layout save/restore ("My VM debug setup")
- Layout presets in dock or workspace context menu
- Auto-layout suggestion: "You have 3 panels open — arrange as debug grid?"

---

10. Modular panel types [BUILT - partial]

Current panel types (PanelType in aios store):
- `app` — any registered VoidTower page
- `embed` — iframe of an http(s) URL
- `odysseus` — future dedicated Odysseus panel type (routing exists, dedicated type planned)
- `stream` — future video/console stream panel type

All VoidTower pages are panel-registered in PANEL_REGISTRY (AiosLayout.tsx). Any page can open as a panel.

Planned new panel types [PLANNED]:

Odysseus panel:
- Dedicated type (`odysseus`) rather than embedding the AI page generically
- Receives prefill queries via postMessage from any other panel
- Maintains conversation state independent of panel position
- Context indicator in panel header: shows the resource the user last interacted with
- Suggested actions based on the context resource

Stream panel [FUTURE]:
- VM console (noVNC / SPICE / WebRTC stream)
- Android instance stream (scrcpy → WebRTC)
- Rendered via dedicated StreamPanel component, not a generic iframe

Inspector panel [PLANNED]:
- Right-side slide-in detail panel for a selected resource
- Sections: Overview, Status, Metrics, Logs, Network, Storage, Backups, AI permissions, Actions, Timeline
- Shown as right-docked column on large screens
- Shown as bottom sheet on phone
- Shown as slide-over on tablet

Future panel modules [FUTURE]:
- GPU passthrough panel (assign GPU to KVM VM)
- Proxmox snapshot panel
- Waydroid controls panel
- Android APK install panel
- DNS zone editor panel
- AI permissions / policy panel
- Backup restore panel

---

11. Resource adapter system [FUTURE]

The resource adapter pattern was planned in the original design but has not yet been built. The current architecture opens VoidTower pages as panels rather than individual resource instances.

Target architecture:

Every hosted resource (Docker app, VM, Android instance, network service) exposes a common adapter interface so Void Mode can open it as a resource window with relevant panels auto-populated.

```typescript
type ResourceType =
  | "docker_app"
  | "custom_docker"
  | "vm"
  | "proxmox_vm"
  | "android"
  | "ai_service"
  | "network_service"
  | "storage"
  | "backup_job"
  | "external_link"

type Resource = {
  id: string
  name: string
  type: ResourceType
  status: "running" | "stopped" | "warning" | "error" | "unknown"
  provider: "docker" | "compose" | "libvirt" | "proxmox" | "waydroid" | "redroid" | "external"
  capabilities: ResourceCapability[]
  aiIntegration: {
    level: "native" | "aware" | "ready" | "none"
    tools: string[]
    permissions: {
      read: string[]
      safeWrite: string[]
      risky: string[]
      dangerous: string[]
    }
  }
  access: {
    web?: string
    console?: string
    stream?: string
    terminal?: boolean
    logs?: boolean
    metrics?: boolean
  }
}

type ResourceAdapter = {
  type: ResourceType
  label: string
  icon: string
  getDefaultWindow(resource: Resource): PanelState
  getAvailablePanels(resource: Resource): string[]
  getQuickActions(resource: Resource): ResourceAction[]
  getAIContext(resource: Resource): AIContext
}
```

What this enables:
- Docker app → opens app details, compose editor, logs, metrics panels
- VM → opens console stream, snapshots, hardware panel
- Android → opens stream, ADB shell, APK install panel
- Network service → opens route manager, DNS, tunnel status

This is the foundation for Void Mode becoming a true resource-aware OS rather than just a windowed page system.

---

12. Odysseus in Void Mode [BUILT - partial]

Current state:
- `/` prefix in the command bar routes queries to Odysseus via postMessage
- `SendToOdysseus` component present in codebase — copies context to clipboard and opens Odysseus URL in new tab or embed panel
- Odysseus integration config (enable/disable, URL, webhook secret) in Settings → Integrations
- SSE event stream at /api/integrations/events

What this means today:
- The user can open the AI page in a Void Mode panel and use the `/ask` command bar prefix to send queries
- "Send to Odysseus" buttons on resource pages copy context and open Odysseus in a new tab
- The Odysseus embed URL can be typed into the command bar to open it as an embed panel beside any resource

Planned additions [PLANNED]:

Dedicated Odysseus panel type:
- Opens as a proper panel in the Void Mode canvas
- Does not require the user to know the Odysseus URL (reads from integration config)
- Prefill supported: any "Send to Odysseus" button sends context directly into the open Odysseus panel instead of clipboard
- Context indicator in panel header: "Context: jellyfin · recent logs · CPU spike"
- Panel remembers conversation state across workspace switches

"Open beside Odysseus" action [PLANNED]:
- One-click on any resource: snap current panel left, open Odysseus panel right, prefill with resource context
- Available from command palette and resource detail page header

AI approval queue [PLANNED]:
- When Odysseus requests a high-risk action, it appears in an approval queue panel
- User can: approve once, deny, approve with time limit, create policy from repeated pattern
- Each decision is audit logged
- Critical approvals create a toast notification and increment the notification bell badge

Future [FUTURE]:

Context bridge:
- The active Odysseus panel is aware of which other panels are open and which resource is focused
- Odysseus can read visible logs, current metrics, and the selected resource's state without the user explicitly sending context
- The context indicator shows exactly what Odysseus can currently see

Inline AI insight chips:
- Optional one-line annotation on resource cards: "Odysseus: high restart count — check OOM events"
- Opt-in per user, off by default

Workspace-aware Odysseus:
- Odysseus knows the current workspace name and can suggest saving/naming it
- Odysseus can suggest opening additional panels based on what the user is working on

---

13. Input model [BUILT]

Mouse support (desktop):
- Drag panels by title bar
- Resize from edges and corners
- Hover tooltips on dock icons
- Click to focus panel, bring to front

Keyboard support:
- Ctrl+K / ⌘K: command bar / command palette
- Ctrl+Shift+V: toggle Tower/Void mode
- Ctrl+1-4: switch workspaces
- Ctrl+Shift+Arrow: snap focused panel
- Ctrl+W: close focused panel
- Ctrl+M: close all panels
- Alt+Tab: cycle visible panels
- Escape: minimize focused panel / close modal / dismiss command bar
- Arrow keys: navigate TV layout tiles, command bar results

Touch support (phone/tablet):
- Phone: sheet panels (full-screen bottom sheet, one visible at a time)
- Phone dock: full-width bottom tab bar with labels
- Phone command bar: FAB button above dock opens bottom sheet
- Tablet: up to 3 floating panels, larger hit targets
- Kiosk: tap to wake, tap PIN pad digits

Planned touch additions [PLANNED]:
- Swipe panel title bar to minimize
- Swipe down on a sheet panel to dismiss
- Long press dock icon for a quick-action menu
- Pinch-to-zoom within terminal panels

Future [FUTURE]:
- Right-click / long-press context menus on panel title bars
- Window drag handles that are larger on tablet (touch-first resize)
- "Snap to grid" option: panels align to an invisible grid when dragged

---

14. Device tier and responsive behavior [BUILT]

Tiers are detected by window width and pointer media queries:

| Tier    | Width       | Pointer    | Layout |
|---------|-------------|------------|--------|
| phone   | <640px      | any        | Sheet panels, bottom dock+tabs, FAB command |
| tablet  | 640–1199px  | any        | Up to 3 floating panels, pill dock |
| desktop | 1200–1919px | fine       | Up to 5 panels, pill dock or vertical strip (≥1400) |
| large   | ≥1920px     | fine       | Up to 8 panels, vertical strip dock |
| tv      | ≥1200px     | coarse+no-hover | TV grid layout |
| kiosk   | ?mode=kiosk | any        | Kiosk auto-cycle layout |

Tier override: set localStorage key `vt-device-mode` to any tier value to force it. Useful for testing TV or kiosk layouts on a desktop.

Mobile use cases (phone tier):
- Check and acknowledge alerts (sheet panel, Alerts page)
- Restart a failing service (sheet panel, Services page)
- View container logs (sheet panel, Containers page)
- Run a manual automation job (sheet panel, Automation page)
- Open a terminal session (sheet panel, Terminal page)
- Switch UI mode to Tower for tables/forms if preferred

---

15. Navigation [BUILT]

Current navigation items in the dock:

Infrastructure:
Dashboard, Services, Containers, VMs, App Vault, Storage, Files

Network:
Network, Proxies, WireGuard, Firewall

Operations:
Backups, Automation, Alerts, Timeline, Terminal

AI / Intelligence:
AI, Models

System:
Capabilities, Diagnostics, Secrets, Security, Audit Log, Tags, Integrations, Updates, Mods, Themes, Settings

Keyboard shortcuts (current):
- Ctrl+K / ⌘K: command bar
- Ctrl+Shift+V: toggle Tower/Void mode
- Ctrl+1-4: workspace switch (Void Mode)
- Ctrl+Shift+Arrow: snap focused panel (Void Mode)
- Ctrl+W: close focused panel (Void Mode)
- Ctrl+M: close all panels (Void Mode)
- Alt+Tab: cycle visible panels (Void Mode)
- Escape: minimize / dismiss

Planned shortcuts [PLANNED]:
- Ctrl+Alt+O: open/focus Odysseus panel
- Ctrl+Alt+T: open Terminal panel
- Ctrl+Alt+A: open App Vault panel
- Ctrl+Alt+V: open VMs panel
- Ctrl+Shift+S: save current workspace as preset
- ?: show keyboard shortcut overlay

---

16. Visual design [BUILT]

Void Mode design language:
- Dark-first
- Glass panels (backdrop-filter: blur, semi-transparent backgrounds)
- Rounded window corners
- Subtle shadows
- Smooth but restrained animations (respects reduced-motion)
- Cyan/blue/violet AI accents
- Clear semantic status colors (green/amber/red)
- High readability at all densities

Current Void Mode chrome tokens (actual values in use):
```css
/* Panel glass */
background: rgba(0, 0, 0, 0.50);
backdrop-filter: blur(24px);
border: 1px solid rgba(255, 255, 255, 0.10);
box-shadow: 0 8px 32px rgba(0,0,0,0.5);

/* Status bar */
background: rgba(0, 0, 0, 0.40);
backdrop-filter: blur(12px);

/* Command bar expanded */
background: rgba(0, 0, 0, 0.75);
backdrop-filter: blur(24px);
border-radius: 14px;
```

Theme tokens (shared with Tower Mode via CSS variables):
```css
--bg-root: #050509
--bg-panel: #0b0d14
--bg-card: #11131d
--bg-elevated: #171a27
--border-subtle: #25283a
--text-primary: #f4f7ff
--text-secondary: #a8b0c3
--text-muted: #687086
--accent-primary: #8b5cf6
--accent-secondary: #06b6d4
--accent-success: #39ff88
--accent-warning: #f59e0b
--accent-danger: #ef4444
--terminal-green: #00ff9c
--terminal-bg: #020403
```

7 built-in themes: VoidTower Default, Blacksite, Ghost Terminal, Deep Grid, Solar Breach, Light Ops, High Contrast.

Animated background presets: Void, Grid, Aurora, Pulse, Noise, Hex, Circuit. 4 glass levels (none, light, medium, heavy).

---

17. AI integration badges [PLANNED]

App Vault cards and resource panels should show an AI integration tier badge.

Tiers:
- AI Native: Odysseus has full tool coverage (deploy, configure, query, control)
- AI Aware: Odysseus can read status and logs but cannot act
- AI Ready: no integration yet but a template exists for one-click wiring
- (none): community app, unknown AI integration

Defined in the App Vault YAML catalog entry:
```yaml
ai_integration:
  level: native  # native / aware / ready / none
  tools: [read_status, read_logs, restart_service]
  description: "Odysseus can inspect and manage this resource."
```

Badge colors: cyan filled = native, blue filled = aware, grey outline = ready, no badge = none.

Resource card example:
```
┌─────────────────────────────────────┐
│ Jellyfin                  AI Native │
│ Media server                        │
│ Running · CPU 4% · RAM 512 MB       │
│                                     │
│ [Open] [Logs] [Ask Odysseus]        │
└─────────────────────────────────────┘
```

---

18. Universal inspector [PLANNED]

Every resource should have a unified inspector panel.

Inspector sections:
- Overview (name, status, type, provider, uptime)
- Status (health check result, last event)
- Metrics (CPU, RAM, network, disk I/O for containers/VMs)
- Logs (embedded log viewer)
- Network (ports, domains, proxy rules)
- Storage (volumes, mounts, disk usage)
- Backups (last backup, last restore test, confidence)
- AI permissions (what Odysseus is allowed to do with this resource)
- Actions (safe actions + danger zone)
- Timeline (recent events for this resource)
- Notes (Markdown notes, pinnable)

Display modes by tier:
- Large/desktop: right-docked column
- Desktop: floating inspector panel
- Tablet: slide-over from the right
- Phone: bottom sheet

---

19. Activity timeline [BUILT - partial]

Current: Timeline.tsx — global audit timeline with category chips, search, outcome filter, paginated scroll.

Every manual and AI action is logged. The timeline is the source of truth for "what changed and when."

Planned additions [PLANNED]:
- Per-resource timeline view in the inspector panel
- Odysseus actions tagged with an AI actor badge (not just generic "api" actor)
- Undo indicator: show which events in the timeline can be rolled back, with a rollback button
- Export: download selected time range as JSON or CSV
- Link event to an incident (if incident mode is built)

Each timeline event should record:
- Who did it (user name or "Odysseus" or "automation")
- What changed
- When it happened
- Which resource was affected
- Whether it can be undone
- Whether AI was involved

---

20. Notification center [BUILT - partial]

Current: NotifBell in AiosStatusBar shows a dropdown. Count is currently static (0).

Notification types to implement:
- Backup completed
- Backup failed
- Container crashed / unhealthy
- VM snapshot created
- Disk almost full (>80% / >90%)
- Tunnel disconnected
- AI action requires confirmation
- App update available
- Security warning (new failed login, session from new IP)
- Automation failed
- Certificate expiring

Planned [PLANNED]:
- Connect notification bell to a real reactive alerts/events store
- Notifications grouped by type (dismiss all of type)
- Notification age / timestamp
- Click-through: clicking a notification opens the relevant panel/page
- Odysseus summary card: "3 things need attention: disk is high, Gitea backup failed, Windows VM has no recent snapshot."

---

21. Accessibility [BUILT - partial]

Current:
- Keyboard navigable throughout (focus states on buttons, links)
- ARIA labels on icon-only buttons in dock, status bar, panel controls
- Reduced motion: animations level "off" setting in theme editor disables transitions
- High Contrast built-in theme
- Visible focus outlines (outline ring on active elements)

Planned [PLANNED]:
- Settings → Appearance → Accessibility subsection:
  - Reduce transparency (remove backdrop-filter glass)
  - Reduce motion (separate from animations, also disables panel drag transitions)
  - High contrast (quick toggle, switches to High Contrast theme)
  - Large controls (increases tap targets and font sizes globally)
  - Prefer stacked layout (forces phone-tier behavior on all device sizes)
  - Disable floating windows on mobile (forces Tower Mode on phone tier)
- Screen-reader-friendly live regions (aria-live) for metric updates and toast notifications
- Color-independent state communication (icon + color, not color alone)
- WCAG AA contrast compliance check in the theme editor for custom colors

---

22. Void Mode workflows [BUILT - partial]

These are the primary Void Mode usage patterns. Some are fully working, some require planned additions.

Workflow 1: VM + Odysseus [PARTIAL]

Current: the user can open VMs as a panel, type the Odysseus embed URL into the command bar to open it as an embed panel beside VMs, and snap both left/right.

Full workflow when planned features are built:
1. User opens VMs panel.
2. User clicks "Open beside Odysseus" or types /ask in the command bar.
3. VMs snaps left, Odysseus panel opens right with VM context prefilled.
4. Odysseus shows: "I can inspect VM metrics, check host load, review recent events, or take a snapshot."

Layout:
```
┌────────────────────────────┬────────────────────────────┐
│ VMs                        │ Odysseus                   │
│ KVM / Proxmox list         │ Context: selected VM       │
└────────────────────────────┴────────────────────────────┘
```

Workflow 2: Debug an app [PARTIAL]

Current: the user can open Containers, a log viewer (LogViewer), and snap panels manually.

Full workflow:
1. User opens App Vault or Containers and finds a failing app.
2. User selects "Debug Layout" preset.
3. Void Mode opens: app details (top-left), logs (bottom-left), metrics (bottom-right), Odysseus (top-right).
4. Odysseus sees the logs and says: "I see repeated permission errors on /config. I can explain them or suggest a fix."

Layout:
```
┌────────────────────────────┬────────────────────────────┐
│ App details                │ Odysseus                   │
├────────────────────────────┼────────────────────────────┤
│ Logs                       │ Metrics                    │
└────────────────────────────┴────────────────────────────┘
```

Workflow 3: Android testing [FUTURE]

Requires: Redroid/Waydroid stream panel type, ADB shell integration.

Layout:
```
┌────────────────────────────┬────────────────────────────┐
│ Android stream             │ Odysseus                   │
├────────────────────────────┴────────────────────────────┤
│ ADB shell / logs                                         │
└──────────────────────────────────────────────────────────┘
```

Workflow 4: Custom app deployment [PLANNED]

1. User opens App Vault panel.
2. User clicks "Deploy Custom".
3. A deployment form panel opens (image, name, ports, volumes, env vars).
4. VoidTower generates a Docker Compose file.
5. The new app appears in the running apps list.
6. Odysseus offers to set up proxy, monitoring, and backup.

Workflow 5: Mobile quick repair [PARTIAL]

1. User opens VoidTower on phone (sheet/tab bar mode).
2. User taps Alerts tab.
3. Reviews alerts, taps one to open details.
4. Taps "Send to Odysseus" — context copied, Odysseus opens.
5. User returns to VoidTower and taps Services, restarts the failing unit.

---

23. Planned future UI areas [FUTURE unless marked PLANNED]

Multi-node / Agent Mode [PLANNED when backend is built]:
- Nodes page: health table for all registered nodes
- Node selector: scope all pages to a specific node
- Node detail page: hardware, metrics, services, containers, ports, events, notes
- Node add wizard: agent URL + join token + test connection
- Per-node metric cards in the dashboard

Maintenance Windows [PLANNED]:
- Maintenance windows page: create/edit with time range, affected resources, suppressed alert categories
- Active maintenance banner in the dashboard and status bar during a window
- Automation policy scoping (only allowed automations run during the window)

Incident Mode [PLANNED]:
- Incidents page: create from alert, attach logs/metrics/services/containers
- Incident detail: timeline, owner, status tracking, notes editor
- Status tracking: Investigating / Identified / Monitoring / Resolved / Postmortem Pending
- Postmortem export: markdown from incident data
- "Send incident to Odysseus" button

Config Drift Detection [PLANNED]:
- Drift view in resource detail pages
- Expected vs. actual diff
- Reconcile button / Accept external change button
- Ignore rule for known differences

Inventory / Asset Database [FUTURE]:
- Per-node hardware card: CPU, RAM, disk, GPU, OS, kernel
- Installed packages list
- Notes field per resource (Markdown, pinnable, searchable)
- Queryable: "which nodes have GPUs", "which services have no backups"

Declarative / GitOps Mode [FUTURE]:
- State export as YAML
- State import with diff preview
- Dry-run apply
- Optional Git sync
- Odysseus drafts state-change PRs instead of acting directly

Policy Engine [PLANNED]:
- Policy rules page: actor + action + resource type + time window + approval requirement
- Policy violation log
- Policy dry-run testing

Plugin Manager (Mods) [PLANNED]:
- Mods.tsx placeholder exists
- List installed plugins with permissions and status
- Plugin install from URL or local file
- Permission review before enable
- Per-plugin audit log

Disaster Recovery [PLANNED]:
- Settings section: export config, import config, emergency admin reset
- Emergency disable panel: one-click disable for Odysseus, automations, webhooks, MCP server

OpenAPI / Developer Tools [PLANNED]:
- API docs page: Swagger/Redoc UI from /api/openapi.json
- Available at /api/docs

Demo / Simulation Mode [PLANNED]:
- Activated via `voidtower --demo`
- Status bar shows "Demo Mode" banner
- All data is synthetic
- No real host operations

---

24. Implementation phase status

Phase 1 — Mode system [DONE]
Tower/Void mode state, visible global toggle, persist per user, layout switcher, shared tokens.

Phase 2 — Void shell [DONE]
Status bar, dock, workspace canvas, notification bell, command bar, animated background.

Phase 3 — Window manager [DONE]
Open/close/drag/resize/minimize/maximize, z-index, snap zones, state persistence.

Phase 4 — Split layouts [DONE - basic]
Split screen manager (left/right snap auto-couple), draggable divider, quarter snaps.
Still needed: bottom drawer layout, three-panel layout, saved presets.

Phase 5 — Resource panels [DONE - as page panels]
All VoidTower pages open as panels. True resource-instance adapters not yet built.

Phase 6 — Responsive / touch [DONE - basic]
Phone/tablet/desktop/large/TV/kiosk device tiers, adaptive dock and command bar.
Still needed: touch drag handles (larger), swipe-to-minimize, right-click context menus.

Phase 7 — AI-native workflows [PARTIAL]
SendToOdysseus component built. Command bar Odysseus prefix built.
Still needed: dedicated Odysseus panel type, context bridge, AI approval queue, context indicator.

Phase 8 — Polish [ONGOING]
7 themes, animated backgrounds, glass levels, 14-param animation editor.
Still needed: accessibility subsection, large controls mode, workspace naming, inspector panel.

---

25. Acceptance criteria [updated]

The UI is complete when:

Core:
- User can switch Tower ↔ Void mode from a visible button and via Ctrl+Shift+V.
- Tower Mode preserves the classic sidebar + topbar + content layout.
- Void Mode operates as a floating-panel web OS on desktop.
- All 29+ navigation sections open as panels in Void Mode.
- Panels persist position, size, and workspace across page reload.
- Device tiers are detected and the correct layout variant is served.

Void Mode panels:
- Panels can be dragged, resized, minimized, maximized, closed, and pinned.
- Panels snap to left-half, right-half, quarter zones, and fullscreen.
- Two snapped panels auto-couple into a split pair with a draggable divider.
- Alt+Tab cycles visible panels on the current workspace.
- Panel cap is enforced per device tier (oldest auto-minimized when exceeded).

Navigation and input:
- Ctrl+K opens command bar with app search, /Odysseus prefix, and URL embed.
- Ctrl+1-4 switches workspaces.
- Ctrl+Shift+Arrow snaps focused panel.
- All keyboard shortcuts work without a mouse.
- Phone tier uses sheet panels and bottom tab bar.
- TV tier uses D-pad-navigable tile grid.
- Kiosk tier auto-cycles tiles and supports PIN wake.

AI and Odysseus:
- SendToOdysseus component present on alerts, containers, services.
- /prefix in command bar routes to Odysseus.
- URL typed in command bar opens as embed panel.

Themes and appearance:
- 7 built-in themes work.
- Theme editor allows live customization of all tokens.
- Animated background presets are selectable.
- Reduced-motion setting disables all transitions.

Accessibility:
- Keyboard navigable throughout.
- ARIA labels on icon-only controls.
- Visible focus states on all interactive elements.
- High Contrast theme is functional.

Planned (before public release):
- Named workspaces with save/restore.
- Dedicated Odysseus panel type with context indicator.
- "Open beside Odysseus" one-click action.
- Inspector panel (right-docked on large screens, slide-over on tablet, sheet on phone).
- AI integration badges on App Vault cards.
- AI approval queue panel.
- Notification bell connected to real alerts store.
- Accessibility subsection in Settings (reduce motion, reduce transparency, large controls).

---

26. Final framing

Tower Mode:
The control tower. Structured, direct, table-driven, administrative. Where you configure the system.

Void Mode:
The living workspace. Modular, AI-native, windowed, touch-friendly, and immersive. Where you live inside it.

The goal:

Open a VM, dock Odysseus beside it, drag a log panel to the bottom, pin metrics to the corner, switch to an Android instance, save the layout, return later and continue exactly where you left off. Everything runs inside the browser. No SSH required. No manual config hunting. Civilization, finally.

---

27. Interactive AI agent background [PLANNED]

The background becomes a living visualization of what AI agents and automations are actually doing.

Architecture:
- A new canvas layer sits between AnimatedBackground and the Void Mode panel canvas. It is transparent to pointer events so panel interaction is never blocked.
- Each running Odysseus agent or active automation job is represented as a small animated character.
- Characters are positioned near "resource nodes" (containers, VMs, services) drawn as faint labeled dots.
- Motion between nodes is driven by SSE events from `/api/integrations/events`: when an agent acts on a resource its character travels to that node.

Visual design:
- Characters: 16×16px animated sprite or SVG. Default: a small glowing dot with a short motion trail. Optional: pixel art bots, geometric shapes, user-configurable emoji.
- Resource nodes: faint circles + label, opacity 0.25 so they don't compete with panels.
- Speech bubbles: small pill labels above characters showing the current action (max 40 chars). Auto-dismiss after 4s.
- Path trails: fading line behind last 8 positions. Configurable on/off.
- Layer dims to 0.4 opacity when a panel is focused, brightens to 1.0 when the canvas is empty.

Configuration (Settings → Appearance → Agent Visualization):
- Enable/disable (off by default)
- Which agents/automations to show (multi-select)
- Avatar style: dot / sprite / emoji picker
- Speech bubbles: on/off
- Path trails: on/off
- Node opacity: slider

Agent inspector popover (click a character):
- Agent name and status (running / idle / paused)
- Current task description
- Last 5 actions with timestamps
- Resource being acted on — click to open its panel
- Pause/resume toggle
- "Open Odysseus with this agent's context" button

---

28. Custom tabs and embeds [PLANNED]

Users can add personal navigation entries that appear alongside built-in pages in both Tower and Void modes.

Tab types:
- **VoidTower page** — any registered route opened as a panel
- **Embed URL** — iframe with the same sandbox as existing embed panels
- **File viewer** — opens a local path in the Files viewer (image, PDF, video, markdown)
- **WebSocket stream** — connects to a ws:// endpoint and renders the text stream in a scrollable terminal-style panel; useful for custom log tails, scripts, bots
- **Custom HTML snippet** — sandboxed inline HTML/CSS/JS; good for custom status widgets

Tab manager (Settings → My Tabs):
- List of custom tabs with name, icon, type, and target
- Add / edit / delete / toggle visibility
- Drag-to-reorder
- Owner can create instance-wide shared tabs visible to all users

Suggested embeds (shown as recommendations in the Add Tab dialog):
- Grafana — CSP relaxed, dark theme param appended automatically
- Netdata — `?mode=embedded` appended
- Homer / Dashy / Homarr — service dashboard iframes
- Jupyter Lab — notebook viewer
- Portainer — with WebSocket passthrough note
- Excalidraw (self-hosted) — whiteboard
- Any running App Vault app — auto-populated list from the catalog

---

29. Full UI customization [PLANNED]

Every visual and navigational element of VoidTower is configurable per-user, with owner-level instance defaults.

Branding (Settings → Appearance → Branding):
- Instance name — replaces "VoidTower" in the sidebar wordmark, page title, and login page
- Logo upload — PNG or SVG; replaces the VT icon in sidebar and browser favicon
- Login background — image shown behind the login card
- Login tagline — one-line text below the login form
- Custom CSS — multi-line field injected as a `<style>` tag; scoped to `:root`
- All stored in backend, served in the initial page load to avoid flash-of-default-branding

Navigation editor (Settings → Navigation):
- Toggle each nav item on/off
- Inline rename per item
- Lucide icon picker per item (searchable icon grid)
- Drag-to-reorder within groups
- Create and rename custom nav groups
- Per-user by default; owner can push as instance default

Menu position & layout (Settings → Navigation → Menu Layout):

```
Left sidebar          — Tower Mode default
Right sidebar         — mirrored, content on left
Top horizontal bar    — nav items as scrollable row
Bottom bar            — same, pinned to bottom edge
Floating pill         — Void Mode dock default
Pop-out drawer        — hidden, slides in on hover/shortcut/button press
Icon-only strip       — icons only, label on hover
```

Animation options: Slide / Fade / Spring / None
Auto-hide: Always visible / Hide on scroll (re-appear on scroll up) / Hide until hovered

---

30. User management and multi-tenancy [PLANNED]

VoidTower supports multiple users on the same instance — household members, teams, or shared self-hosted environments.

User profile (per account):
- Display name, avatar (upload or pick from generated set), email
- Personal note shown on the instance Users page

Per-user customization:
- Default UI mode (Tower / Void), default workspace preset, theme, navigation layout
- Custom tabs and dock shortcuts
- Language preference (future i18n hook)

Per-user AI endpoint:
- Odysseus URL, personal API key (never exposed to other users), default model, system prompt prefix
- Usage stats (tokens this month — informational)
- Owner sets a shared fallback endpoint used when no personal endpoint is configured

Household member onboarding wizard:
1. Owner creates account (name, email, role)
2. System generates a setup link (24h TTL)
3. New user sets password, sees welcome screen
4. Welcome screen: "Choose a starter layout" — Minimal / Media center / Developer / AI workspace
5. User lands in a personalized VoidTower

User groups:
- Create named groups (Family, Admins, Media, etc.)
- Assign users to groups
- Resource-level permissions per group (Media group can see Jellyfin + Files, not Firewall + Secrets)
- Group-level default layout presets

Guest access:
- Owner generates a guest link: configurable scope (which pages), read-only vs interact, expiry (1h / 24h / 7d / no expiry)
- Guest sessions tracked in audit log
- Secrets, Security, and Settings pages never visible to guests regardless of scope

---

31. Proxy management — full nginx capabilities [PLANNED]

Current state: create-only (domain + upstream + SSL toggle). No edit, no delete, no advanced options.

Proxy rule form (create and edit):

Basic tab:
- Domain/subdomain, upstream URL (with test button), SSL toggle (Let's Encrypt), active toggle

Headers tab:
- Strip X-Frame-Options (enable for Void Mode iframe embedding)
- Strip CSP frame directives
- Add custom request and response headers (key-value list)

Access tab:
- Basic auth (username + hashed password stored in nginx conf)
- Rate limiting (off / 10 / 30 / 60 / 120 req/min / custom)
- IP allowlist (CIDR list)

Advanced tab:
- WebSocket passthrough
- Gzip compression
- Cache static assets (off / 1h / 24h / 7d)
- Client max body size
- Custom nginx location block (raw text; syntax-highlighted, validated before save)

Presets (one-click bundles):
- **Iframe embed** — strip X-Frame-Options + CSP frame directives
- **Secure internal** — force HTTPS + HSTS, block direct IP
- **Upload app** — 10 GB max body, long proxy timeout, gzip off
- **WebSocket app** — upgrade headers, disable buffering
- **Rate limited public** — 30 req/min, X-Real-IP forwarding
- **Auth-gated** — basic auth required, SSL enforced

AI recommendations:
When a proxy is created for an App Vault app VoidTower checks the YAML catalog for known requirements and shows a dismissable suggestion:
- Portainer → WebSocket preset
- Grafana → Iframe embed + CSP relax
- Nextcloud → Upload app + large body
- Authentik → Secure internal + long timeout

Proxy health dashboard:
- Domain, upstream, SSL validity (badge: valid / expiring <14d / expired), upstream reachability, last check, avg response time
- Manual "test now" per row
- "Renew certificate" button triggers acme.sh/Certbot renewal + nginx reload

Wildcard routing:
- `*.home.domain.tld` → route to `app-name.configured-domain.tld → <upstream>` auto-mapped from running App Vault apps
- Requires wildcard SSL cert (separate flow in the Let's Encrypt UI)

---

32. Search expansion [PLANNED]

Full-text, multi-scope search accessible from Ctrl+K.

Search scopes (all queried simultaneously, results grouped by type):

```
Pages          — nav items by name (current)
Containers     — name, image, status
Services       — unit name, description
VMs            — VM name, ID
App Vault      — app name, category, description
Timeline       — action description, actor, resource name
Secrets        — name only, never value
Automations    — job name, schedule
Files          — file/directory name (shallow index, not full-text file content)
Tags           — tag name, color label
Notes          — note text (when notes feature is built)
```

Result display:
- Grouped by scope with scope icon
- Keyboard: arrows navigate, Enter opens, Tab jumps to next group
- Each result: icon, name, type badge, status indicator
- Opens the resource in the most relevant panel

Filters:
- Scope chips: click to limit results to one type
- Status: running / stopped / error
- Date range (for timeline results)

Saved searches:
- `!name` prefix saves a query; recall with `!name` from an empty command bar
- Listed in a short section at the top of the empty command bar

Odysseus inline search:
- `/` prefix sends to Odysseus (existing behavior)
- First 3 sentences of response shown as a result card inline
- Tab to enter card, Enter opens Odysseus panel with query pre-filled

---

33. AI Creative Studio — Odysseus voidlink deep integration [PLANNED]

VoidTower becomes the local hub for AI content creation, model management, and autonomous agent workflows.

AI Studio page:
- New top-level nav item under AI/Intelligence
- Four columns at desktop width: Generation, Models, Pipelines, Agents
- Opens as a Void Mode panel; defaults to two-thirds width on large screens

Image generation panel:
- Prompt + negative prompt, workflow picker (txt2img / img2img / inpainting / upscale / controlnet)
- Backend selector: ComfyUI or Stable Diffusion WebUI (URL from Settings → Integrations)
- Sampler, steps, CFG, seed — sensible defaults, expandable advanced section
- Built-in viewer with zoom, pan, download, copy prompt, "Send to Odysseus"
- Gallery sidebar: last 50 generations with prompt tooltip; click to re-open
- Batch queue: multiple prompts, progress bar per item

Video generation panel:
- Input: text prompt or image (drag file or paste from image panel)
- Duration, FPS, resolution selectors
- Backend: ComfyUI video workflows or dedicated server (SVD, CogVideoX, AnimateDiff)
- Built-in player with frame scrubber; filmstrip preview while generating

TTS panel:
- Text input (plain text or paste from STT output)
- Voice picker from installed model's voice list
- Speed, pitch, energy sliders
- Backends: Kokoro, Coqui TTS, Piper (auto-detected)
- Built-in audio player with waveform visualization
- Save to Files; voice cloning: upload 10–30s reference clip → fine-tune new voice entry

STT panel:
- Upload audio/video or record from browser microphone (MediaRecorder)
- Model picker: whisper-tiny / base / small / medium / large
- Output: plain text or SRT subtitle file; auto-detect language or specify
- "Send to Odysseus" with "summarize" pre-prompt; "Send to TTS" for narration

Content pipeline editor:
- Visual node graph built on the Automation backend with new AI node types
- Node types: Text Prompt, Image Gen, Video Gen, TTS, STT, Upload File, Save to Files, Send to Odysseus, HTTP Request, Shell Command, Wait, If/Else
- Named pipeline templates, one-click run, run history (inputs, outputs, duration, token cost)
- Example: "Daily AI briefing" — fetch RSS → Odysseus summarize → TTS → save MP3 to Files

3D generation panel:
- Input: text prompt or image (TripoSR, Zero123++, Shap-E via local API)
- Three.js viewer: orbit, wireframe, environment light, export GLB/OBJ/STL
- Progress: point cloud → mesh → textured reconstruction steps shown live

MCP tool inspector panel:
- Loads Odysseus tool manifest from `/api/integrations/odysseus/manifest`
- Lists all MCP tools with name, description, parameter schema
- Input form auto-generated from JSON schema; Execute button calls tool via Odysseus
- Recent call history per tool; useful for testing/debugging MCP integrations

Local agent terminal panel (panel type: `agent-terminal`):
- Runs any CLI AI coding agent in a PTY (Claude Code, Aider, GPT-Engineer, custom)
- Context injection buttons in panel header: "Add open file", "Add container logs", "Add service status", "Add selected text from another panel"
- Context shown as a collapsible sidebar of injected items, each removable
- Session persistence: re-opens with same session within workspace persist window
- Multiple agent terminals can run simultaneously on different workspaces

Inbuilt media viewer panel types:
- `viewer-image` — zoom, pan, next/prev, EXIF metadata sidebar
- `viewer-video` — player with SRT subtitle sidecar, chapter markers, PiP
- `viewer-audio` — waveform, auto-generated chapters via Whisper if available
- `viewer-pdf` — text selection, page jump, bookmark sidebar
- `viewer-3d` — Three.js model viewer (see 3D generation panel)
- `viewer-notebook` — `.ipynb` viewer showing cells + outputs read-only; "Open in Jupyter" button if Jupyter is in App Vault
- All openable from: Files page, AI Studio output, App Vault downloads, drag-and-drop onto Void canvas

Automation library:
- Curated templates accessible from Automation page and Pipeline editor
- Categories: Content Creation, Infrastructure, Monitoring, AI Workflows, Media Processing
- Example templates: "Daily blog draft", "Auto-caption video", "Alert → TTS summary", "Backup report", "Image batch variants"
- One-click install, prompts for variable substitution (paths, model names, thresholds)
- Community import: paste a template URL to install from a remote JSON manifest (no code execution — manifest describes automation config only)
