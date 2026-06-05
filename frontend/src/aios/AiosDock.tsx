import { useState } from 'react'
import {
  LayoutDashboard, Server, Container, Package, Bell,
  HardDrive, Network, Terminal, ClipboardList, Settings,
  Lock, BrainCircuit, FolderOpen, Globe, Cpu, Stethoscope,
  KeyRound, History, Flame, Zap, Wifi, Monitor, Tag, Palette,
  ArrowUpCircle, PlugZap, Puzzle,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useAiosStore } from '@/aios/store/aios'
import type { DeviceTier } from '@/aios/hooks/useDeviceTier'

// ── Nav items — mirrors NAV_GROUPS from Sidebar but flattened ─────────────────

interface DockItem {
  key: string
  icon: LucideIcon
  label: string
}

const DOCK_ITEMS: DockItem[] = [
  { key: 'dashboard',    icon: LayoutDashboard, label: 'Dashboard'    },
  { key: 'alerts',       icon: Bell,            label: 'Alerts'       },
  { key: 'timeline',     icon: History,         label: 'Timeline'     },
  { key: 'services',     icon: Server,          label: 'Services'     },
  { key: 'containers',   icon: Container,       label: 'Containers'   },
  { key: 'vms',          icon: Monitor,         label: 'VMs'          },
  { key: 'apps',         icon: Package,         label: 'App Vault'    },
  { key: 'ai',           icon: BrainCircuit,    label: 'AI'           },
  { key: 'models',       icon: HardDrive,       label: 'Models'       },
  { key: 'network',      icon: Network,         label: 'Network'      },
  { key: 'proxies',      icon: Globe,           label: 'Proxies'      },
  { key: 'wireguard',    icon: Wifi,            label: 'WireGuard'    },
  { key: 'firewall',     icon: Flame,           label: 'Firewall'     },
  { key: 'storage',      icon: HardDrive,       label: 'Storage'      },
  { key: 'backups',      icon: HardDrive,       label: 'Backups'      },
  { key: 'files',        icon: FolderOpen,      label: 'Files'        },
  { key: 'security',     icon: Lock,            label: 'Security'     },
  { key: 'secrets',      icon: KeyRound,        label: 'Secrets'      },
  { key: 'audit',        icon: ClipboardList,   label: 'Audit Log'    },
  { key: 'automation',   icon: Zap,             label: 'Automation'   },
  { key: 'terminal',     icon: Terminal,        label: 'Terminal'     },
  { key: 'capabilities', icon: Cpu,             label: 'Capabilities' },
  { key: 'diagnostics',  icon: Stethoscope,     label: 'Diagnostics'  },
  { key: 'themes',       icon: Palette,         label: 'Themes'       },
  { key: 'updates',      icon: ArrowUpCircle,   label: 'Updates'      },
  { key: 'mods',         icon: Puzzle,          label: 'Mods'         },
  { key: 'integrations', icon: PlugZap,         label: 'Integrations' },
  { key: 'tags',         icon: Tag,             label: 'Tags'         },
  { key: 'settings',     icon: Settings,        label: 'Settings'     },
]

// Export for AiosCommandBar re-use
export { DOCK_ITEMS }
export type { DockItem }

// ── Icon / label maps ─────────────────────────────────────────────────────────

export const LABEL_MAP: Record<string, string> =
  Object.fromEntries(DOCK_ITEMS.map((d) => [d.key, d.label]))

export const ICON_MAP: Record<string, LucideIcon> =
  Object.fromEntries(DOCK_ITEMS.map((d) => [d.key, d.icon]))

// ── Props ─────────────────────────────────────────────────────────────────────

export interface AiosDockProps {
  tier: DeviceTier
  /** Height of status bar — used to inset vertical dock position (default 28) */
  statusBarH?: number
  /** Total dock height in px (default 56 for desktop, 64 for phone) */
  dockH?: number
  /**
   * Optional callback called when a panel should be opened.
   * If omitted the dock calls openPanel from the aios store directly.
   */
  onOpen?: (key: string) => void
}

// ── Default geometry ──────────────────────────────────────────────────────────

const DEFAULT_PANEL_W = 900
const DEFAULT_PANEL_H = 580

function defaultPanelGeo() {
  const vw = window.innerWidth
  const vh = window.innerHeight
  return {
    x: Math.max(20, (vw - DEFAULT_PANEL_W) / 2),
    y: Math.max(40, (vh - DEFAULT_PANEL_H) / 2),
    w: Math.min(DEFAULT_PANEL_W, vw - 40),
    h: Math.min(DEFAULT_PANEL_H, vh - 80),
  }
}

// ── AiosDock ──────────────────────────────────────────────────────────────────

export default function AiosDock({
  tier,
  statusBarH = 28,
  dockH: dockHProp,
  onOpen,
}: AiosDockProps) {
  const { panels, activeWorkspace, openPanel, focusPanel, restorePanel } = useAiosStore()
  const [tooltip, setTooltip] = useState<string | null>(null)

  const isPhone = tier === 'phone'
  const isTv    = tier === 'tv' || tier === 'kiosk'

  // Tablet portrait + phone → horizontal bottom. Desktop/large → depends on width.
  const isVertical =
    (tier === 'desktop' || tier === 'large') && typeof window !== 'undefined' && window.innerWidth >= 1400

  const dockH = dockHProp ?? (isPhone ? 64 : isTv ? 72 : 56)

  // Panels on the active workspace
  const wsPanels = panels.filter((p) => p.workspaceIndex === activeWorkspace)
  const openKeys = new Set(
    wsPanels.filter((p) => p.layoutMode !== 'minimized').map((p) => p.component),
  )
  const minimizedMap = new Map(
    wsPanels.filter((p) => p.layoutMode === 'minimized').map((p) => [p.component, p.id]),
  )

  const handleClick = (item: DockItem) => {
    const minimizedId = minimizedMap.get(item.key)
    if (minimizedId) {
      restorePanel(minimizedId)
      focusPanel(minimizedId)
      return
    }

    const openPanel_ = wsPanels.find((p) => p.component === item.key && p.layoutMode !== 'minimized')
    if (openPanel_) {
      focusPanel(openPanel_.id)
      return
    }

    if (onOpen) {
      onOpen(item.key)
    } else {
      const geo = defaultPanelGeo()
      openPanel({
        type: 'app',
        component: item.key,
        title: item.label,
        icon: '⬡',
        layoutMode: 'floating',
        ...geo,
        savedX: geo.x, savedY: geo.y, savedW: geo.w, savedH: geo.h,
        pinned: false,
        workspaceIndex: activeWorkspace,
      })
    }
  }

  // ── Container style ────────────────────────────────────────────────────────

  const glassBase: React.CSSProperties = {
    background: 'rgba(0,0,0,0.50)',
    backdropFilter: 'blur(24px)',
    WebkitBackdropFilter: 'blur(24px)',
    border: '1px solid rgba(255,255,255,0.10)',
    boxShadow: '0 8px 32px rgba(0,0,0,0.5), 0 2px 8px rgba(0,0,0,0.3)',
  }

  const containerStyle: React.CSSProperties = isVertical
    ? {
        // Tablet landscape / large desktop: left edge vertical strip
        position: 'fixed', left: 0, top: statusBarH, bottom: 0,
        width: dockH,
        display: 'flex', flexDirection: 'column',
        alignItems: 'center', justifyContent: 'flex-start',
        paddingTop: 12, gap: 4, paddingBottom: 12,
        zIndex: 9998, overflowY: 'auto',
        ...glassBase,
        borderRight: '1px solid rgba(255,255,255,0.10)',
        borderTop: 'none', borderBottom: 'none', borderLeft: 'none',
        borderRadius: 0, boxShadow: '2px 0 12px rgba(0,0,0,0.4)',
      }
    : isPhone
      ? {
          // Phone: full-width bottom tab bar with labels
          position: 'fixed', bottom: 0, left: 0, right: 0,
          height: dockH,
          display: 'flex', flexDirection: 'row',
          alignItems: 'center', justifyContent: 'space-around',
          paddingInline: 4,
          zIndex: 9998,
          ...glassBase,
          borderBottom: 'none', borderLeft: 'none', borderRight: 'none',
          borderRadius: 0,
        }
      : {
          // Desktop/tablet: centered pill
          position: 'fixed', bottom: 8, left: '50%', transform: 'translateX(-50%)',
          height: dockH,
          display: 'flex', flexDirection: 'row',
          alignItems: 'center',
          gap: 4, paddingInline: 12,
          zIndex: 9998,
          ...glassBase,
          borderRadius: 20,
          maxWidth: '90vw', overflowX: 'auto',
        }

  const iconSize = isTv ? 26 : isPhone ? 22 : 18
  const btnSize  = isTv ? 56 : isPhone ? 44 : 48

  return (
    <div style={containerStyle}>
      {DOCK_ITEMS.map((item) => {
        const Icon = item.icon
        const isOpen      = openKeys.has(item.key)
        const isMinimized = minimizedMap.has(item.key)

        return (
          <div key={item.key} style={{ position: 'relative', flexShrink: 0 }}>
            <button
              onClick={() => handleClick(item)}
              onMouseEnter={() => setTooltip(item.key)}
              onMouseLeave={() => setTooltip(null)}
              aria-label={item.label}
              aria-pressed={isOpen}
              style={{
                width:  isPhone ? `${Math.floor(100 / Math.min(DOCK_ITEMS.length, 7))}vw` : btnSize,
                height: btnSize,
                minWidth: 44, minHeight: 44,
                display: 'flex', flexDirection: 'column',
                alignItems: 'center', justifyContent: 'center', gap: 4,
                background: isOpen ? 'rgba(255,255,255,0.10)' : 'none',
                border: 'none', cursor: 'pointer',
                borderRadius: isPhone ? 0 : 12,
                color: isOpen ? 'var(--accent-primary)' : 'rgba(255,255,255,0.55)',
                transition: 'color 0.15s, background 0.15s, transform 0.1s',
                outline: isOpen
                  ? '2px solid rgba(var(--accent-primary-rgb, 139,92,246), 0.45)'
                  : 'none',
                outlineOffset: 2,
              }}
              onMouseDown={(e) => {
                ;(e.currentTarget as HTMLElement).style.transform = 'scale(0.9)'
              }}
              onMouseUp={(e) => {
                ;(e.currentTarget as HTMLElement).style.transform = ''
              }}
            >
              <Icon size={iconSize} />

              {/* Phone: label */}
              {isPhone && (
                <span style={{ fontSize: 9, color: 'inherit', lineHeight: 1 }}>
                  {item.label}
                </span>
              )}

              {/* Active / minimized indicator dot */}
              {(isOpen || isMinimized) && (
                <div style={{
                  position: 'absolute', bottom: isPhone ? 4 : 4, left: '50%',
                  transform: 'translateX(-50%)',
                  width: 4, height: 4, borderRadius: '50%',
                  background: isMinimized ? 'var(--accent-warning)' : 'var(--accent-primary)',
                }} />
              )}
            </button>

            {/* Tooltip — desktop only, on hover */}
            {tooltip === item.key && !isPhone && (
              <div style={{
                position: 'absolute',
                ...(isVertical
                  ? { left: dockH + 6, top: '50%', transform: 'translateY(-50%)' }
                  : { bottom: btnSize + 8, left: '50%', transform: 'translateX(-50%)' }),
                background: 'rgba(0,0,0,0.85)',
                backdropFilter: 'blur(8px)',
                WebkitBackdropFilter: 'blur(8px)',
                border: '1px solid rgba(255,255,255,0.12)',
                borderRadius: 6, padding: '4px 9px',
                fontSize: 11, color: 'var(--text-primary)',
                whiteSpace: 'nowrap', pointerEvents: 'none',
                zIndex: 10002,
                boxShadow: '0 4px 12px rgba(0,0,0,0.4)',
              }}>
                {item.label}
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}
