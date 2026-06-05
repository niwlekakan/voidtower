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

interface DockItem {
  key: string
  icon: LucideIcon
  label: string
  requires?: string
}

// Flat list of all nav items — mirrors NAV_GROUPS in Sidebar but unrolled
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

const LABEL_MAP: Record<string, string> = Object.fromEntries(DOCK_ITEMS.map((d) => [d.key, d.label]))
const ICON_MAP: Record<string, LucideIcon> = Object.fromEntries(DOCK_ITEMS.map((d) => [d.key, d.icon]))
export { DOCK_ITEMS, LABEL_MAP, ICON_MAP }

interface Props {
  tier: DeviceTier
  dockH: number
  statusBarH: number
  onOpen: (key: string) => void
}

export default function AiosDock({ tier, dockH, statusBarH, onOpen }: Props) {
  const { panels, activeWorkspace, restorePanel, focusPanel } = useAiosStore()
  const [tooltip, setTooltip] = useState<string | null>(null)

  const workspacePanels = panels.filter((p) => p.workspaceIndex === activeWorkspace)
  const openKeys = new Set(workspacePanels.filter((p) => p.layoutMode !== 'minimized').map((p) => p.component))
  const minimizedPanels = workspacePanels.filter((p) => p.layoutMode === 'minimized')

  const isVertical = tier === 'large' || (tier === 'desktop' && window.innerWidth >= 1400)
  const isPhone = tier === 'phone'

  const containerStyle: React.CSSProperties = isVertical
    ? {
        position: 'fixed', left: 0, top: statusBarH, bottom: 0,
        width: dockH, display: 'flex', flexDirection: 'column',
        alignItems: 'center', justifyContent: 'flex-start',
        paddingTop: 12, gap: 4,
        background: 'var(--bg-panel)', borderRight: '1px solid var(--border-subtle)',
        zIndex: 9998, overflowY: 'auto',
      }
    : {
        position: 'fixed', bottom: 0, left: 0, right: 0,
        height: dockH, display: 'flex', flexDirection: 'row',
        alignItems: 'center', justifyContent: 'center',
        gap: isPhone ? 0 : 2,
        background: 'var(--bg-panel)', borderTop: '1px solid var(--border-subtle)',
        zIndex: 9998, overflowX: 'auto', paddingInline: 8,
      }

  const handleClick = (item: DockItem) => {
    // If a minimized panel matches, restore it
    const minimized = minimizedPanels.find((p) => p.component === item.key)
    if (minimized) {
      restorePanel(minimized.id)
      focusPanel(minimized.id)
      return
    }
    // If already open, focus it
    const open = workspacePanels.find((p) => p.component === item.key && p.layoutMode !== 'minimized')
    if (open) {
      focusPanel(open.id)
      return
    }
    onOpen(item.key)
  }

  return (
    <div style={containerStyle}>
      {DOCK_ITEMS.map((item) => {
        const Icon = item.icon
        const isOpen = openKeys.has(item.key)
        const isMinimized = minimizedPanels.some((p) => p.component === item.key)

        return (
          <div key={item.key} style={{ position: 'relative', flexShrink: 0 }}>
            <button
              onClick={() => handleClick(item)}
              onMouseEnter={() => setTooltip(item.key)}
              onMouseLeave={() => setTooltip(null)}
              aria-label={item.label}
              aria-pressed={isOpen}
              style={{
                width: isPhone ? `${100 / Math.min(DOCK_ITEMS.length, 8)}vw` : 44,
                height: isPhone ? dockH : 44,
                display: 'flex', flexDirection: 'column',
                alignItems: 'center', justifyContent: 'center', gap: 3,
                background: 'none', border: 'none', cursor: 'pointer',
                borderRadius: isPhone ? 0 : 8,
                color: isOpen ? 'var(--accent-primary)' : 'var(--text-muted)',
                transition: 'color 0.15s, background 0.15s',
                position: 'relative',
              }}
            >
              <Icon size={isPhone ? 20 : 18} />
              {isPhone && (
                <span style={{ fontSize: 10, color: 'inherit' }}>{item.label}</span>
              )}

              {/* Active dot */}
              {(isOpen || isMinimized) && (
                <div style={{
                  width: 4, height: 4, borderRadius: '50%',
                  background: isMinimized ? 'var(--accent-warning)' : 'var(--accent-primary)',
                  animation: isMinimized ? 'pulse 2s infinite' : undefined,
                  position: isPhone ? 'absolute' : 'static',
                  bottom: isPhone ? 4 : undefined,
                }} />
              )}
            </button>

            {/* Tooltip */}
            {tooltip === item.key && !isPhone && (
              <div style={{
                position: 'absolute',
                ...(isVertical
                  ? { left: dockH + 4, top: '50%', transform: 'translateY(-50%)' }
                  : { bottom: dockH + 4, left: '50%', transform: 'translateX(-50%)' }),
                background: 'var(--bg-elevated)',
                border: '1px solid var(--border-subtle)',
                borderRadius: 6, padding: '4px 8px',
                fontSize: 11, color: 'var(--text-primary)',
                whiteSpace: 'nowrap', pointerEvents: 'none', zIndex: 10002,
                boxShadow: '0 4px 12px rgba(0,0,0,0.3)',
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
