import { useEffect, useCallback, Component, useState } from 'react'
import type { ReactNode, ErrorInfo } from 'react'
import { LayoutGrid } from 'lucide-react'
import { useMetrics } from '@/hooks/useMetrics'
import { useAiosStore } from '@/aios/store/aios'
import { useDeviceTier } from '@/aios/hooks/useDeviceTier'
import { LABEL_MAP } from '@/aios/AiosDock'
import AiosStatusBar, { STATUS_BAR_H } from '@/aios/AiosStatusBar'
import AiosPanel from '@/aios/AiosPanel'
import AiosDock from '@/aios/AiosDock'
import AiosCommandBar from '@/aios/AiosCommandBar'
import AiosSplitDivider from '@/aios/AiosSplitDivider'
import AiosTvLayout from '@/aios/AiosTvLayout'
import AiosKioskLayout from '@/aios/AiosKioskLayout'
import AiosOdysseus from '@/aios/AiosOdysseus'
import AiosConfirm from '@/aios/AiosConfirm'
import { AiosAskPopup } from '@/aios/AiosAskPopup'
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
import AiosInspectorPanel from '@/aios/AiosInspectorPanel'
import { PanelShell } from '@/aios/PanelShell'

// ── Native panel imports ─────────────────────────────────────────────────────
import NativeDashboardPanel    from '@/aios/panels/dashboard'
import NativeServicesPanel     from '@/aios/panels/services'
import NativeContainersPanel   from '@/aios/panels/containers'
import NativeAlertsPanel       from '@/aios/panels/alerts'
import NativeProxiesPanel      from '@/aios/panels/proxies'
import NativeFirewallPanel     from '@/aios/panels/firewall'
import NativeAutomationPanel   from '@/aios/panels/automation'
import NativeSecretsPanel      from '@/aios/panels/secrets'
import NativeWireGuardPanel    from '@/aios/panels/wireguard'
import NativeBackupsPanel      from '@/aios/panels/backups'
import NativeVMsPanel          from '@/aios/panels/vms'
import NativeNetworkPanel      from '@/aios/panels/network'
import NativeStoragePanel      from '@/aios/panels/storage'
import NativeFilesPanel        from '@/aios/panels/files'
import NativeModelsPanel       from '@/aios/panels/models'
import NativeTimelinePanel     from '@/aios/panels/timeline'
import NativeTagsPanel         from '@/aios/panels/tags'
import NativeSecurityPanel     from '@/aios/panels/security'
import NativeIntegrationsPanel from '@/aios/panels/integrations'
import NativeSettingsPanel     from '@/aios/panels/settings'

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

// Native panels render directly without PanelShell (they use NativePanelShell internally)
const NATIVE_PANEL_REGISTRY: Record<string, React.ComponentType> = {
  dashboard:    NativeDashboardPanel,
  services:     NativeServicesPanel,
  containers:   NativeContainersPanel,
  alerts:       NativeAlertsPanel,
  proxies:      NativeProxiesPanel,
  firewall:     NativeFirewallPanel,
  automation:   NativeAutomationPanel,
  secrets:      NativeSecretsPanel,
  wireguard:    NativeWireGuardPanel,
  backups:      NativeBackupsPanel,
  vms:          NativeVMsPanel,
  network:      NativeNetworkPanel,
  storage:      NativeStoragePanel,
  files:        NativeFilesPanel,
  models:       NativeModelsPanel,
  timeline:     NativeTimelinePanel,
  tags:         NativeTagsPanel,
  security:     NativeSecurityPanel,
  integrations: NativeIntegrationsPanel,
  settings:     NativeSettingsPanel,
}

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

function PanelContent({ component, panelId, title }: { component: string; panelId: string; title: string }) {
  if (component === 'odysseus') {
    return (
      <PanelErrorBoundary>
        <AiosOdysseus panelId={panelId} />
      </PanelErrorBoundary>
    )
  }
  if (component === 'inspector') {
    return (
      <PanelErrorBoundary>
        <AiosInspectorPanel />
      </PanelErrorBoundary>
    )
  }
  const NativePage = NATIVE_PANEL_REGISTRY[component]
  if (NativePage) return (
    <PanelErrorBoundary>
      <NativePage />
    </PanelErrorBoundary>
  )
  const Page = PANEL_REGISTRY[component]
  if (!Page) return (
    <div style={{ padding: 24, fontSize: 13, color: 'var(--text-muted)' }}>
      Unknown page: {component}
    </div>
  )
  return (
    <PanelShell title={title}>
      <PanelErrorBoundary>
        <Page />
      </PanelErrorBoundary>
    </PanelShell>
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

// ── Snap edge indicators ────────────────────────────────────────────────────
// Thin glowing strips at screen edges that reveal snap zones on hover.
// Always mounted so users discover them; opacity is near-zero at rest.

interface EdgeZone { id: string; style: React.CSSProperties; label: string }

function SnapEdgeIndicators() {
  const [hovered, setHovered] = useState<string | null>(null)

  const accent = 'var(--accent-primary, #8b5cf6)'
  const zones: EdgeZone[] = [
    { id: 'left',         label: '⬡ left half',       style: { left: 0,    top: '25%',   width: 6,   height: '50%' } },
    { id: 'right',        label: 'right half ⬡',      style: { right: 0,   top: '25%',   width: 6,   height: '50%' } },
    { id: 'top-left',     label: '↖ top-left',        style: { left: 0,    top: 0,        width: 40,  height: 40   } },
    { id: 'top-right',    label: 'top-right ↗',       style: { right: 0,   top: 0,        width: 40,  height: 40   } },
    { id: 'bottom-left',  label: '↙ bottom-left',     style: { left: 0,    bottom: 0,     width: 40,  height: 40   } },
    { id: 'bottom-right', label: 'bottom-right ↘',    style: { right: 0,   bottom: 0,     width: 40,  height: 40   } },
  ]

  return (
    <>
      {zones.map((z) => {
        const active = hovered === z.id
        return (
          <div
            key={z.id}
            onMouseEnter={() => setHovered(z.id)}
            onMouseLeave={() => setHovered(null)}
            style={{
              position: 'absolute',
              ...z.style,
              zIndex: 1,
              borderRadius: 4,
              background: active ? `${accent}30` : `${accent}08`,
              border: `1px solid ${active ? `${accent}60` : `${accent}15`}`,
              boxShadow: active ? `0 0 12px ${accent}40` : 'none',
              transition: 'all 0.18s ease',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              pointerEvents: 'auto',
              cursor: 'default',
            }}
          >
            {active && (
              <span style={{
                fontSize: 9,
                color: accent,
                whiteSpace: 'nowrap',
                padding: '2px 4px',
                background: 'rgba(0,0,0,0.7)',
                borderRadius: 3,
                pointerEvents: 'none',
              }}>
                {z.label}
              </span>
            )}
          </div>
        )
      })}
    </>
  )
}

// ── Default panel geometry by tier ─────────────────────────────────────────

function defaultGeometry(
  tier: string, index: number,
  statusH: number, dockH: number, dockLeft: number,
): { x: number; y: number; w: number; h: number } {
  const vw = window.innerWidth
  const vh = window.innerHeight
  const canvas = vh - statusH - dockH
  const usableW = vw - dockLeft

  if (tier === 'phone') return { x: 0, y: statusH, w: vw, h: canvas }

  const w = tier === 'tablet' ? Math.min(usableW * 0.9, 860) : Math.min(usableW * 0.65, 960)
  const h = tier === 'tablet' ? Math.min(canvas * 0.9, 620) : Math.min(canvas * 0.75, 680)
  const offset = index * 28
  const x = Math.min(dockLeft + (usableW - w) / 2 + offset, vw - w - 20)
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
    tileMode, tileTrees, toggleTileMode, splitPanel,
  } = useAiosStore()
  const askOpen = useAiosStore((s) => s.askOpen)

  const isPhone = tier === 'phone'
  const isTv = tier === 'tv'
  const isKiosk = tier === 'kiosk'
  const statusBarH = isPhone ? 28 : isTv ? 52 : STATUS_BAR_H
  const dockH = isPhone ? 68 : 62
  const isVerticalDock = !isPhone && window.innerWidth >= 1400
  const dockLeft = isVerticalDock ? dockH : 0

  // Panels visible on current workspace
  const workspacePanels = panels.filter((p) => p.workspaceIndex === activeWorkspace)

  const openApp = useCallback((key: string) => {
    const existing = workspacePanels.find((p) => p.component === key && p.layoutMode !== 'minimized')
    if (existing) { focusPanel(existing.id); return }

    const index = workspacePanels.filter((p) => p.layoutMode !== 'minimized').length
    const geo = defaultGeometry(tier, index, statusBarH, dockH, dockLeft)
    const mode = isPhone ? 'sheet' : 'floating'

    openPanel({
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

  const openOdysseus = useCallback((query?: string) => {
    const existing = panels.find((p) => p.component === 'odysseus')
    if (existing) {
      if (existing.layoutMode === 'minimized') {
        useAiosStore.getState().restorePanel(existing.id)
      }
      focusPanel(existing.id)
      return
    }
    const index = workspacePanels.filter((p) => p.layoutMode !== 'minimized').length
    const geo = defaultGeometry(tier, index, statusBarH, dockH, dockLeft)
    const mode = isPhone ? 'sheet' : 'floating'
    openPanel({
      type: 'odysseus',
      component: 'odysseus',
      title: 'Odysseus',
      icon: '',
      layoutMode: mode,
      workspaceIndex: activeWorkspace,
      pinned: false,
      savedX: geo.x, savedY: geo.y, savedW: geo.w, savedH: geo.h,
      ...geo,
    })
    // If a query was given, we pass it — AiosOdysseus reads initialQuery from URL params
    // via a postMessage so the iframe can pick it up when ready
    if (query) {
      setTimeout(() => {
        window.postMessage({ type: 'vt-command', text: query }, '*')
      }, 300)
    }
  }, [panels, workspacePanels, focusPanel, openPanel, tier, activeWorkspace, isPhone])

  // Keep store dims in sync with actual rendered bar/dock heights
  useEffect(() => {
    useAiosStore.getState().setDims(statusBarH, dockH, dockLeft)
  }, [statusBarH, dockH])

  // Re-apply tile layout whenever tiling state or dimensions change
  useEffect(() => {
    if (tileMode) useAiosStore.getState().applyTileLayout()
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tileMode, tileTrees[activeWorkspace], statusBarH, dockH, dockLeft])

  // Global keyboard shortcuts
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      // Ctrl+Shift+Space — open Ask AI popup
      if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key === ' ') {
        e.preventDefault()
        useAiosStore.getState().setAskOpen(true)
        return
      }

      // Ctrl+Alt+O — toggle Odysseus panel
      if ((e.ctrlKey || e.metaKey) && e.altKey && e.key.toLowerCase() === 'o') {
        e.preventDefault()
        const existing = panels.find((p) => p.component === 'odysseus')
        if (existing && existing.layoutMode !== 'minimized' && focusedId === existing.id) {
          useAiosStore.getState().minimizePanel(existing.id)
        } else {
          openOdysseus()
        }
        return
      }

      // Ctrl+Alt+B — snap focused panel left, open Odysseus right
      if ((e.ctrlKey || e.metaKey) && e.altKey && e.key.toLowerCase() === 'b') {
        e.preventDefault()
        useAiosStore.getState().openBesideOdysseus()
        return
      }

      // Ctrl+Alt+T — toggle tile mode
      if ((e.ctrlKey || e.metaKey) && e.altKey && e.key.toLowerCase() === 't') {
        e.preventDefault()
        useAiosStore.getState().toggleTileMode()
        return
      }

      // Ctrl+Alt+H / Ctrl+Alt+V — split panel horizontally / vertically (tile mode only)
      if ((e.ctrlKey || e.metaKey) && e.altKey && e.key.toLowerCase() === 'h') {
        e.preventDefault()
        const state = useAiosStore.getState()
        if (state.tileMode && state.focusedId) {
          state.splitPanel(state.focusedId, 'h', {
            type: 'app',
            component: 'terminal',
            title: 'Terminal',
            icon: '',
            layoutMode: 'floating',
            x: 0, y: 0, w: 800, h: 500,
            savedX: 0, savedY: 0, savedW: 800, savedH: 500,
            pinned: false,
            workspaceIndex: state.activeWorkspace,
          })
        }
        return
      }

      if ((e.ctrlKey || e.metaKey) && e.altKey && e.key.toLowerCase() === 'v') {
        e.preventDefault()
        const state = useAiosStore.getState()
        if (state.tileMode && state.focusedId) {
          state.splitPanel(state.focusedId, 'v', {
            type: 'app',
            component: 'terminal',
            title: 'Terminal',
            icon: '',
            layoutMode: 'floating',
            x: 0, y: 0, w: 800, h: 500,
            savedX: 0, savedY: 0, savedW: 800, savedH: 500,
            pinned: false,
            workspaceIndex: state.activeWorkspace,
          })
        }
        return
      }

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
  }, [panels, focusedId, workspacePanels, focusPanel, setWorkspace, openOdysseus, tileMode, splitPanel])

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
    <div style={{ position: 'fixed', inset: 0, ['--aios-status-h' as any]: `${statusBarH}px`, ['--aios-dock-h' as any]: `${dockH}px`, ['--aios-dock-left' as any]: `${dockLeft}px` }}>
      <AnimatedBackground />
      <AiosStatusBar tier={tier} />

      {/* Panel canvas */}
      <div style={canvasStyle}>
        {/* Snap zone edge indicators (desktop only) */}
        {(tier === 'desktop' || tier === 'large') && <SnapEdgeIndicators />}

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
                  : <PanelContent component={panel.component} panelId={panel.id} title={panel.title} />
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

      {/* Tile mode toggle button — fixed to bottom-right of status bar */}
      <button
        title={tileMode ? 'Exit tile mode (Ctrl+Alt+T)' : 'Enter tile mode (Ctrl+Alt+T)'}
        onClick={() => toggleTileMode()}
        style={{
          position: 'fixed',
          top: Math.round(statusBarH / 2) - 10,
          right: 12,
          zIndex: 10000,
          width: 20,
          height: 20,
          background: 'none',
          border: 'none',
          cursor: 'pointer',
          padding: 0,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          color: tileMode ? 'var(--accent-primary, #6366f1)' : 'rgba(255,255,255,0.35)',
          transition: 'color 0.15s',
        }}
        onMouseEnter={(e) => { if (!tileMode) (e.currentTarget as HTMLButtonElement).style.color = 'rgba(255,255,255,0.7)' }}
        onMouseLeave={(e) => { if (!tileMode) (e.currentTarget as HTMLButtonElement).style.color = 'rgba(255,255,255,0.35)' }}
      >
        <LayoutGrid size={14} />
      </button>

      <AiosAskPopup open={askOpen} onClose={() => useAiosStore.getState().setAskOpen(false)} />
      <NotificationToasts />
      <CommandPalette />
      <AiosConfirm />
      {user?.force_password_change && <ForcePasswordChange />}
    </div>
  )
}
