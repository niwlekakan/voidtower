import { useState } from 'react'
import {
  LayoutDashboard, Server, Container, Package, Bell,
  HardDrive, Network, Terminal, ClipboardList, Settings,
  Lock, BrainCircuit, FolderOpen, Globe, Cpu, Stethoscope,
  KeyRound, History, Flame, Zap, Wifi, Monitor, Tag, Palette,
  ArrowUpCircle, PlugZap, Puzzle, Bot, MoreHorizontal, Activity, LayoutPanelTop,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useAiosStore } from '@/aios/store/aios'
import type { DeviceTier } from '@/aios/hooks/useDeviceTier'
import { useNavConfigStore, resolvedNavItems } from '@/store/navConfig'

// ── Nav items — mirrors NAV_GROUPS from Sidebar but flattened ─────────────────

interface DockItem {
  key: string
  icon: LucideIcon
  label: string
  aiLevel?: 'native' | 'aware' | 'ready'
  /** Primary items are always prominent; secondary items are visually lighter */
  primary?: boolean
}

// Phone dock shows only these 5 keys + a "+more" button
const PHONE_PRIMARY_KEYS = ['dashboard', 'odysseus', 'terminal', 'apps', 'alerts'] as const

const DOCK_ITEMS: DockItem[] = [
  // ── PRIMARY (core daily-use, always visible at full opacity) ────────────────
  { key: 'dashboard',    icon: LayoutDashboard, label: 'Dashboard',    primary: true },
  { key: 'odysseus',     icon: Bot,             label: 'Odysseus',     primary: true,  aiLevel: 'native'  },
  { key: 'terminal',     icon: Terminal,        label: 'Terminal',     primary: true,  aiLevel: 'aware'   },
  { key: 'apps',         icon: Package,         label: 'App Vault',    primary: true },
  { key: 'containers',   icon: Container,       label: 'Containers',   primary: true,  aiLevel: 'aware'   },
  { key: 'vms',          icon: Monitor,         label: 'VMs',          primary: true },
  { key: 'files',        icon: FolderOpen,      label: 'Files',        primary: true },
  { key: 'alerts',       icon: Bell,            label: 'Alerts',       primary: true },
  { key: 'settings',     icon: Settings,        label: 'Settings',     primary: true },
  // ── SECONDARY (accessible but not crowding primary) ─────────────────────────
  { key: 'services',     icon: Server,          label: 'Services',     aiLevel: 'aware'  },
  { key: 'ai',           icon: BrainCircuit,    label: 'AI',           aiLevel: 'native' },
  { key: 'models',       icon: HardDrive,       label: 'Models'       },
  { key: 'network',      icon: Network,         label: 'Network'      },
  { key: 'proxies',      icon: Globe,           label: 'Proxies'      },
  { key: 'proxmox',     icon: Server,          label: 'Proxmox'      },
  { key: 'wireguard',    icon: Wifi,            label: 'WireGuard'    },
  { key: 'firewall',     icon: Flame,           label: 'Firewall'     },
  { key: 'storage',      icon: HardDrive,       label: 'Storage'      },
  { key: 'backups',      icon: HardDrive,       label: 'Backups'      },
  { key: 'security',     icon: Lock,            label: 'Security'     },
  { key: 'secrets',      icon: KeyRound,        label: 'Secrets'      },
  { key: 'audit',        icon: ClipboardList,   label: 'Audit Log'    },
  { key: 'automation',   icon: Zap,             label: 'Automation'   },
  { key: 'agents',       icon: Activity,        label: 'Agents'       },
  { key: 'tabs',         icon: LayoutPanelTop,  label: 'Custom Tabs'  },
  { key: 'tags',         icon: Tag,             label: 'Tags'         },
  { key: 'integrations', icon: PlugZap,         label: 'Integrations' },
  { key: 'updates',      icon: ArrowUpCircle,   label: 'Updates'      },
  { key: 'mods',         icon: Puzzle,          label: 'Mods'         },
  { key: 'capabilities', icon: Cpu,             label: 'Capabilities' },
  { key: 'diagnostics',  icon: Stethoscope,     label: 'Diagnostics'  },
  { key: 'themes',       icon: Palette,         label: 'Themes'       },
  { key: 'timeline',     icon: History,         label: 'Timeline'     },
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
  const navConfigItems = useNavConfigStore((s) => s.items)
  const navResolved = resolvedNavItems(navConfigItems)
  const navMap = Object.fromEntries(navResolved.map((n) => [n.id, n]))

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
    background: 'rgba(8,6,18,0.92)',
    backdropFilter: 'blur(48px)',
    WebkitBackdropFilter: 'blur(48px)',
    border: '1px solid rgba(255,255,255,0.08)',
    boxShadow: '0 8px 32px rgba(0,0,0,0.6), 0 2px 8px rgba(0,0,0,0.4)',
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

  const iconSize          = isTv ? 26 : isPhone ? 22 : 20
  const iconSizeSecondary = isTv ? 22 : isPhone ? 18 : 16
  const btnSize           = isTv ? 56 : isPhone ? 44 : 52

  // Separator between primary and secondary sections
  const separator = isVertical ? (
    <div key="__sep" style={{
      width: '70%', height: 1,
      background: 'rgba(255,255,255,0.12)',
      margin: '4px 0', flexShrink: 0, alignSelf: 'center',
    }} />
  ) : (
    <div key="__sep" style={{
      width: 1, height: '60%',
      background: 'rgba(255,255,255,0.12)',
      margin: '0 4px', flexShrink: 0, alignSelf: 'center',
    }} />
  )

  // Group divider for secondary overflow section
  const groupDivider = (key: string) => isVertical ? (
    <div key={key} style={{
      width: '60%', height: 1,
      background: 'rgba(255,255,255,0.08)',
      margin: '4px 0', flexShrink: 0, alignSelf: 'center',
    }} />
  ) : (
    <div key={key} style={{
      width: 1, height: '50%',
      background: 'rgba(255,255,255,0.08)',
      margin: '0 2px', flexShrink: 0, alignSelf: 'center',
    }} />
  )

  // Apply nav config: filter hidden items and apply label overrides
  const applyNavConfig = (items: DockItem[]): DockItem[] =>
    items
      .filter((d) => navMap[d.key]?.visible !== false)
      .map((d) => ({ ...d, label: navMap[d.key]?.label ?? d.label }))

  // Items to render — phone shows only the 5 phone-primary keys + more button;
  // desktop/tablet shows primaries with labels + secondary overflow.
  const renderItems = () => {
    if (isPhone) {
      const phoneItems = applyNavConfig(
        DOCK_ITEMS.filter((d) => (PHONE_PRIMARY_KEYS as readonly string[]).includes(d.key))
      )
      return (
        <>
          {phoneItems.map((item) => renderButton(item))}
          {/* "+more" button opens the command bar */}
          <div key="__more" style={{ position: 'relative', flexShrink: 0 }}>
            <button
              onClick={() => window.dispatchEvent(new Event('vt-open-command-bar'))}
              aria-label="More"
              style={{
                width: `${Math.floor(100 / (phoneItems.length + 1))}vw`,
                height: btnSize,
                minWidth: 44, minHeight: 44,
                display: 'flex', flexDirection: 'column',
                alignItems: 'center', justifyContent: 'center', gap: 4,
                background: 'none', border: 'none', cursor: 'pointer',
                borderRadius: 0,
                color: 'rgba(255,255,255,0.45)',
                transition: 'color 0.15s, background 0.15s',
              }}
            >
              <MoreHorizontal size={iconSize} />
              <span style={{ fontSize: 9, color: 'inherit', lineHeight: 1 }}>More</span>
            </button>
          </div>
        </>
      )
    }

    const primaries   = applyNavConfig(DOCK_ITEMS.filter((d) => d.primary))
    const secondaries = applyNavConfig(DOCK_ITEMS.filter((d) => !d.primary))

    // Insert dividers every 3 secondary items for scannability
    const secondaryNodes: React.ReactNode[] = []
    secondaries.forEach((item, i) => {
      if (i > 0 && i % 3 === 0) secondaryNodes.push(groupDivider(`__gsep${i}`))
      secondaryNodes.push(renderButton(item, true))
    })

    return (
      <>
        {primaries.map((item) => renderButton(item, false))}
        {separator}
        {secondaryNodes}
      </>
    )
  }

  const renderButton = (item: DockItem, isSecondary = false) => {
    const Icon = item.icon
    const isOpen      = openKeys.has(item.key)
    const isMinimized = minimizedMap.has(item.key)
    const baseOpacity = isSecondary && !isOpen && !isMinimized ? 0.55 : 1
    // On desktop (non-phone, non-vertical), primary items always show label
    const showLabelAlways = !isPhone && !isVertical && !isSecondary && !isTv

    return (
      <div
        key={item.key}
        style={{ position: 'relative', flexShrink: 0, opacity: baseOpacity, transition: 'opacity 0.15s' }}
        onMouseEnter={(e) => {
          if (isSecondary) (e.currentTarget as HTMLElement).style.opacity = '1'
        }}
        onMouseLeave={(e) => {
          if (isSecondary && !isOpen && !isMinimized) (e.currentTarget as HTMLElement).style.opacity = String(baseOpacity)
        }}
      >
        <button
          onClick={() => handleClick(item)}
          aria-label={item.label}
          aria-pressed={isOpen}
          style={{
            width:  isPhone ? `${Math.floor(100 / Math.min(DOCK_ITEMS.length, 7))}vw` : isSecondary ? 44 : btnSize,
            height: isSecondary ? 44 : btnSize,
            minWidth: 44, minHeight: 44,
            display: 'flex', flexDirection: 'column',
            alignItems: 'center', justifyContent: 'center',
            gap: showLabelAlways ? 3 : 4,
            background: isOpen ? 'rgba(139,92,246,0.12)' : 'none',
            border: 'none', cursor: 'pointer',
            borderRadius: isPhone ? 0 : 10,
            color: isOpen ? 'var(--accent-primary)' : isSecondary ? 'rgba(255,255,255,0.45)' : 'rgba(255,255,255,0.7)',
            transition: 'color 0.15s, background 0.15s, transform 0.1s',
            outline: isOpen
              ? '1px solid rgba(139,92,246,0.4)'
              : 'none',
            outlineOffset: 2,
            padding: showLabelAlways ? '6px 4px 4px' : undefined,
          }}
          onMouseDown={(e) => {
            ;(e.currentTarget as HTMLElement).style.transform = 'scale(0.92)'
          }}
          onMouseUp={(e) => {
            ;(e.currentTarget as HTMLElement).style.transform = ''
          }}
          onMouseEnter={(e) => {
            ;(e.currentTarget as HTMLElement).style.color = 'rgba(255,255,255,0.95)'
            setTooltip(item.key)
          }}
          onMouseLeave={(e) => {
            ;(e.currentTarget as HTMLElement).style.color = isOpen
              ? 'var(--accent-primary)'
              : isSecondary ? 'rgba(255,255,255,0.45)' : 'rgba(255,255,255,0.7)'
            setTooltip(null)
          }}
        >
          <Icon size={isSecondary ? iconSizeSecondary : iconSize} />

          {/* Always-visible label for primary items on desktop */}
          {showLabelAlways && (
            <span style={{
              fontSize: 10,
              lineHeight: 1.2,
              color: 'inherit',
              textAlign: 'center',
              maxWidth: btnSize - 4,
              overflow: 'hidden',
              display: '-webkit-box',
              WebkitLineClamp: 2,
              WebkitBoxOrient: 'vertical',
              wordBreak: 'break-word',
            } as React.CSSProperties}>
              {item.label}
            </span>
          )}

          {/* Active / minimized indicator dot */}
          {(isOpen || isMinimized) && (
            <div style={{
              position: 'absolute', bottom: 3, left: '50%',
              transform: 'translateX(-50%)',
              width: 4, height: 4, borderRadius: '50%',
              background: isMinimized ? 'var(--accent-warning)' : 'var(--accent-primary)',
            }} />
          )}
        </button>

        {/* Tooltip — desktop secondary items + vertical dock, on hover */}
        {tooltip === item.key && !isPhone && (isSecondary || isVertical) && (
          <div style={{
            position: 'absolute',
            ...(isVertical
              ? { left: dockH + 6, top: '50%', transform: 'translateY(-50%)' }
              : { bottom: (isSecondary ? 44 : btnSize) + 8, left: '50%', transform: 'translateX(-50%)' }),
            background: 'rgba(8,6,18,0.95)',
            backdropFilter: 'blur(8px)',
            WebkitBackdropFilter: 'blur(8px)',
            border: '1px solid rgba(255,255,255,0.10)',
            borderRadius: 6, padding: '4px 9px',
            fontSize: 11, color: 'var(--text-primary)',
            whiteSpace: 'nowrap', pointerEvents: 'none',
            zIndex: 10002,
            boxShadow: '0 4px 12px rgba(0,0,0,0.5)',
          }}>
            {item.label}
          </div>
        )}
      </div>
    )
  }

  return (
    <div style={containerStyle}>
      {renderItems()}
    </div>
  )
}
