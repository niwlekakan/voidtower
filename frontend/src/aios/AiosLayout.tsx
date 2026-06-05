import { useEffect, useCallback, Component } from 'react'
import type { ReactNode, ErrorInfo } from 'react'
import { useMetrics } from '@/hooks/useMetrics'
import { useAiosStore, newPanelId } from '@/aios/store/aios'
import { useDeviceTier } from '@/aios/hooks/useDeviceTier'
import { LABEL_MAP } from '@/aios/AiosDock'
import AiosStatusBar from '@/aios/AiosStatusBar'
import AiosPanel from '@/aios/AiosPanel'
import AiosDock from '@/aios/AiosDock'
import AiosCommandBar from '@/aios/AiosCommandBar'
import AiosSplitDivider from '@/aios/AiosSplitDivider'
import AiosTvLayout from '@/aios/AiosTvLayout'
import AiosKioskLayout from '@/aios/AiosKioskLayout'
import AnimatedBackground from '@/components/ui/AnimatedBackground'
import NotificationToasts from '@/components/ui/NotificationToasts'
import CommandPalette from '@/components/ui/CommandPalette'
import ForcePasswordChange from '@/components/ui/ForcePasswordChange'
import { useAuthStore } from '@/store/auth'

// ── Page imports (mirrored from App.tsx) ────────────────────────────────────
import DashboardPage from '@/pages/Dashboard'
import ServicesPage from '@/pages/Services'
import ContainersPage from '@/pages/Containers'
import AppVaultPage from '@/pages/AppVault'
import AlertsPage from '@/pages/Alerts'
import BackupsPage from '@/pages/Backups'
import StoragePage from '@/pages/Storage'
import NetworkPage from '@/pages/Network'
import TerminalPage from '@/pages/Terminal'
import AuditPage from '@/pages/Audit'
import AIPage from '@/pages/AI'
import FilesPage from '@/pages/Files'
import ProxiesPage from '@/pages/Proxies'
import SecurityPage from '@/pages/Security'
import SettingsPage from '@/pages/Settings'
import AutomationPage from '@/pages/Automation'
import FirewallPage from '@/pages/Firewall'
import TimelinePage from '@/pages/Timeline'
import SecretsPage from '@/pages/Secrets'
import CapabilitiesPage from '@/pages/Capabilities'
import DiagnosticsPage from '@/pages/Diagnostics'
import WireGuardPage from '@/pages/WireGuard'
import VMsPage from '@/pages/VMs'
import TagsPage from '@/pages/Tags'
import ThemesPage from '@/pages/Themes'
import ModelsPage from '@/pages/Models'
import UpdatesPage from '@/pages/Updates'
import IntegrationsPage from '@/pages/Integrations'
import ModsPage from '@/pages/Mods'

// ── Error boundary — prevents one bad panel from blanking the whole page ─────

class PanelErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null }
  static getDerivedStateFromError(error: Error) { return { error } }
  componentDidCatch(error: Error, info: ErrorInfo) { console.error('[AiosPanel]', error, info) }
  render() {
    if (this.state.error) {
      const err = this.state.error as Error
      return (
        <div style={{ padding: 24, color: 'var(--accent-danger)', fontSize: 13 }}>
          <strong>Panel error</strong>
          <pre style={{ marginTop: 8, fontSize: 11, opacity: 0.7, whiteSpace: 'pre-wrap' }}>{err.message}</pre>
          <p style={{ marginTop: 8, fontSize: 11, color: 'var(--text-muted)' }}>Check the browser console (F12) for details.</p>
          <button onClick={() => this.setState({ error: null })}
            style={{ marginTop: 12, padding: '4px 12px', borderRadius: 6, background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', cursor: 'pointer', color: 'var(--text-primary)', fontSize: 12 }}>
            Retry
          </button>
        </div>
      )
    }
    return this.props.children
  }
}

// ── Panel component registry ─────────────────────────────────────────────────
// Direct render — no nested Router (React Router v6 forbids them).
// Pages use the outer BrowserRouter context for any navigation they trigger.

const PANEL_REGISTRY: Record<string, React.ComponentType> = {
  dashboard:    DashboardPage,
  services:     ServicesPage,
  containers:   ContainersPage,
  apps:         AppVaultPage,
  alerts:       AlertsPage,
  backups:      BackupsPage,
  storage:      StoragePage,
  network:      NetworkPage,
  terminal:     TerminalPage,
  audit:        AuditPage,
  ai:           AIPage,
  files:        FilesPage,
  proxies:      ProxiesPage,
  security:     SecurityPage,
  settings:     SettingsPage,
  automation:   AutomationPage,
  firewall:     FirewallPage,
  timeline:     TimelinePage,
  secrets:      SecretsPage,
  capabilities: CapabilitiesPage,
  diagnostics:  DiagnosticsPage,
  wireguard:    WireGuardPage,
  vms:          VMsPage,
  tags:         TagsPage,
  themes:       ThemesPage,
  models:       ModelsPage,
  updates:      UpdatesPage,
  integrations: IntegrationsPage,
  mods:         ModsPage,
}

function PanelContent({ component }: { component: string }) {
  const Page = PANEL_REGISTRY[component]
  if (!Page) return (
    <div style={{ padding: 24, fontSize: 13, color: 'var(--text-muted)' }}>
      Unknown page: {component}
    </div>
  )
  return (
    <PanelErrorBoundary>
      <Page />
    </PanelErrorBoundary>
  )
}

function EmbedContent({ url }: { url: string }) {
  return (
    <iframe
      src={url}
      title="Embedded app"
      style={{ width: '100%', height: '100%', border: 'none', display: 'block' }}
      allow="microphone; camera; clipboard-write; fullscreen"
      sandbox="allow-scripts allow-same-origin allow-forms allow-popups"
    />
  )
}

// ── Default panel geometry by tier ─────────────────────────────────────────

function defaultGeometry(tier: string, index: number): { x: number; y: number; w: number; h: number } {
  const vw = window.innerWidth
  const vh = window.innerHeight
  const statusH = tier === 'phone' ? 24 : 28
  const dockH = tier === 'phone' ? 64 : 56
  const canvas = vh - statusH - dockH

  if (tier === 'phone') return { x: 0, y: statusH, w: vw, h: canvas }

  const w = tier === 'tablet' ? Math.min(vw * 0.9, 860) : Math.min(vw * 0.65, 960)
  const h = tier === 'tablet' ? Math.min(canvas * 0.9, 620) : Math.min(canvas * 0.75, 680)
  const offset = index * 28
  const x = Math.min((vw - w) / 2 + offset, vw - w - 20)
  const y = Math.min(statusH + 40 + offset, vh - h - dockH - 20)
  return { x, y, w, h }
}

// ── Main layout ─────────────────────────────────────────────────────────────

export default function AiosLayout() {
  useMetrics()
  const user = useAuthStore((s) => s.user)
  const tier = useDeviceTier()
  const {
    panels, activeWorkspace, focusedId, splitPair,
    openPanel, focusPanel, setWorkspace,
  } = useAiosStore()

  const isPhone = tier === 'phone'
  const isTv = tier === 'tv'
  const isKiosk = tier === 'kiosk'
  const statusBarH = isPhone ? 28 : isTv ? 52 : 36
  const dockH = isPhone ? 68 : 62
  const isVerticalDock = !isPhone && window.innerWidth >= 1400

  // Panels visible on current workspace
  const workspacePanels = panels.filter((p) => p.workspaceIndex === activeWorkspace)

  const openApp = useCallback((key: string) => {
    const existing = workspacePanels.find((p) => p.component === key && p.layoutMode !== 'minimized')
    if (existing) { focusPanel(existing.id); return }

    const index = workspacePanels.filter((p) => p.layoutMode !== 'minimized').length
    const geo = defaultGeometry(tier, index)
    const mode = isPhone ? 'sheet' : 'floating'

    openPanel({
      id: newPanelId(),
      type: key.startsWith('http') ? 'embed' : 'app',
      component: key,
      title: LABEL_MAP[key] ?? key,
      icon: '',
      layoutMode: mode,
      workspaceIndex: activeWorkspace,
      pinned: false,
      savedX: geo.x, savedY: geo.y, savedW: geo.w, savedH: geo.h,
      ...geo,
    })
  }, [workspacePanels, focusPanel, openPanel, tier, activeWorkspace, isPhone])

  const openOdysseus = useCallback((query: string) => {
    const existing = panels.find((p) => p.type === 'odysseus')
    if (existing) {
      focusPanel(existing.id)
      // postMessage prefill
      const iframe = document.querySelector<HTMLIFrameElement>(`[data-panel-id="${existing.id}"] iframe`)
      iframe?.contentWindow?.postMessage({ type: 'prefill', query }, '*')
      return
    }
    openApp('ai')
  }, [panels, focusPanel, openApp])

  // Global keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Workspace switching
      if ((e.ctrlKey || e.metaKey) && ['1','2','3','4'].includes(e.key)) {
        e.preventDefault()
        setWorkspace((parseInt(e.key) - 1) as 0|1|2|3)
      }

      const focused = panels.find((p) => p.id === focusedId)
      if (!focused) return

      // Snap shortcuts
      if ((e.ctrlKey || e.metaKey) && e.shiftKey) {
        const { snapPanel } = useAiosStore.getState()
        if (e.key === 'ArrowLeft')  { e.preventDefault(); snapPanel(focused.id, 'left-half') }
        if (e.key === 'ArrowRight') { e.preventDefault(); snapPanel(focused.id, 'right-half') }
        if (e.key === 'ArrowUp')    { e.preventDefault(); snapPanel(focused.id, 'fullscreen') }
        if (e.key === 'ArrowDown')  { e.preventDefault(); useAiosStore.getState().restorePanel(focused.id) }
      }

      if (e.key === 'Escape')                            useAiosStore.getState().minimizePanel(focused.id)
      if ((e.ctrlKey || e.metaKey) && e.key === 'w')    { e.preventDefault(); useAiosStore.getState().closePanel(focused.id) }
      if ((e.ctrlKey || e.metaKey) && e.key === 'm')    { e.preventDefault(); useAiosStore.getState().closeAll() }

      // Alt+Tab: cycle panels
      if (e.altKey && e.key === 'Tab') {
        e.preventDefault()
        const visible = workspacePanels.filter((p) => p.layoutMode !== 'minimized')
        if (visible.length < 2) return
        const idx = visible.findIndex((p) => p.id === focusedId)
        const next = visible[(idx + 1) % visible.length]
        focusPanel(next.id)
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [panels, focusedId, workspacePanels, focusPanel, setWorkspace])

  // TV and kiosk use dedicated layouts
  if (isTv)    return <AiosTvLayout onOpen={openApp} />
  if (isKiosk) return <AiosKioskLayout onOpen={openApp} />

  const canvasStyle: React.CSSProperties = {
    position: 'fixed',
    top: statusBarH,
    left: isVerticalDock ? dockH : 0,
    right: 0,
    bottom: isPhone ? dockH : dockH,
    overflow: 'hidden',
  }

  return (
    <div style={{ position: 'fixed', inset: 0 }}>
      <AnimatedBackground />
      <AiosStatusBar tier={tier} />

      {/* Panel canvas */}
      <div style={canvasStyle}>
        {/* Sort panels: pinned last (highest), then by zIndex */}
        {[...workspacePanels]
          .sort((a, b) => (a.pinned ? 1 : 0) - (b.pinned ? 1 : 0) || a.zIndex - b.zIndex)
          .map((panel) => (
            <AiosPanel
              key={panel.id}
              panel={panel}
              tier={tier}
              statusBarH={statusBarH}
              dockH={dockH}
            >
              <div data-panel-id={panel.id} style={{ height: '100%' }}>
                {panel.type === 'embed'
                  ? <EmbedContent url={panel.component} />
                  : <PanelContent component={panel.component} />
                }
              </div>
            </AiosPanel>
          ))
        }

        {/* Split divider */}
        {splitPair && !isPhone && (
          <AiosSplitDivider statusBarH={0} dockH={0} />
        )}
      </div>

      <AiosDock
        tier={tier}
        dockH={dockH}
        statusBarH={statusBarH}
        onOpen={openApp}
      />

      <AiosCommandBar
        tier={tier}
        statusBarH={statusBarH}
        dockH={dockH}
        onOpen={openApp}
        onOdysseus={openOdysseus}
      />

      <NotificationToasts />
      <CommandPalette />
      {user?.force_password_change && <ForcePasswordChange />}
    </div>
  )
}
