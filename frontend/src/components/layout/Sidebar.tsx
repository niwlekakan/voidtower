import { useState, useEffect } from 'react'
import { NavLink, useNavigate } from 'react-router-dom'
import {
  LayoutDashboard, Server, Container, Package, Bell,
  HardDrive, Network, Terminal, ClipboardList, Settings,
  ChevronLeft, ChevronRight, LogOut, Shield, Lock, BrainCircuit, FolderOpen, Globe, X, KeyRound, History, Flame, Zap, Wifi, Monitor, Tag, ArrowUpCircle, PlugZap, Puzzle, Palette,
} from 'lucide-react'
import { clsx } from 'clsx'
import { useAuthStore } from '@/store/auth'
import { api } from '@/api/client'
import { useNavConfigStore, resolvedNavItems, resolvedNavGroups } from '@/store/navConfig'

interface NavItem {
  to: string
  icon: React.ElementType
  label: string
  requires?: string  // capability id — item hidden when capability is absent
}

interface NavGroup {
  label: string
  items: NavItem[]
}

const NAV_GROUPS: NavGroup[] = [
  {
    label: 'Overview',
    items: [
      { to: '/dashboard', icon: LayoutDashboard, label: 'Dashboard' },
      { to: '/alerts',    icon: Bell,            label: 'Alerts'    },
      { to: '/timeline',  icon: History,         label: 'Timeline'  },
    ],
  },
  {
    label: 'Resources',
    items: [
      { to: '/services',   icon: Server,    label: 'Services',  requires: 'systemd' },
      { to: '/containers', icon: Container, label: 'Containers'                     },
      { to: '/vms',        icon: Monitor,   label: 'VMs',       requires: 'kvm'     },
      { to: '/proxmox',    icon: Server,    label: 'Proxmox'                        },
      { to: '/apps',       icon: Package,   label: 'App Vault'                      },
    ],
  },
  {
    label: 'AI',
    items: [
      { to: '/ai',     icon: BrainCircuit, label: 'Workspace' },
      { to: '/models', icon: HardDrive,    label: 'Models'    },
    ],
  },
  {
    label: 'Network',
    items: [
      { to: '/network',   icon: Network, label: 'Network'                        },
      { to: '/proxies',   icon: Globe,   label: 'Proxies'                        },
      { to: '/wireguard', icon: Wifi,    label: 'WireGuard', requires: 'wireguard' },
      { to: '/firewall',  icon: Flame,   label: 'Firewall',  requires: 'ufw'       },
    ],
  },
  {
    label: 'Data',
    items: [
      { to: '/storage', icon: HardDrive,  label: 'Storage' },
      { to: '/backups', icon: HardDrive,  label: 'Backups' },
      { to: '/files',   icon: FolderOpen, label: 'Files'   },
    ],
  },
  {
    label: 'Security',
    items: [
      { to: '/security', icon: Lock,          label: 'Security'  },
      { to: '/secrets',  icon: KeyRound,      label: 'Secrets'   },
      { to: '/audit',    icon: ClipboardList, label: 'Audit Log' },
    ],
  },
  {
    label: 'Ops',
    items: [
      { to: '/automation', icon: Zap,      label: 'Automation' },
      { to: '/terminal',   icon: Terminal, label: 'Terminal'   },
      { to: '/tags',       icon: Tag,      label: 'Tags'       },
    ],
  },
  {
    label: 'System',
    items: [
      { to: '/integrations', icon: PlugZap,       label: 'Integrations' },
      { to: '/updates',      icon: ArrowUpCircle, label: 'Updates'      },
      { to: '/mods',         icon: Puzzle,        label: 'Mods'         },
      { to: '/themes',       icon: Palette,       label: 'Themes'       },
      { to: '/settings',     icon: Settings,      label: 'Settings'     },
    ],
  },
]

// Flat lookup: id -> NavItem definition (icon, route, requires)
const NAV_ITEMS_BY_ID: Record<string, NavItem> = Object.fromEntries(
  NAV_GROUPS.flatMap(g => g.items).map(item => [item.to.replace(/^\//, ''), item])
)

export default function Sidebar() {
  const [collapsed, setCollapsed] = useState(false)
  const [available, setAvailable] = useState<Set<string> | null>(null)
  const [instanceName, setInstanceName] = useState('VoidTower')
  const { user, logout } = useAuthStore()
  const navigate = useNavigate()
  const navItems = useNavConfigStore((s) => s.items)
  const storedGroups = useNavConfigStore((s) => s.navGroups)
  const resolved = resolvedNavItems(navItems)
  const activeGroups = resolvedNavGroups(storedGroups)
  // Build a lookup: id -> { label, visible }
  const navMap = Object.fromEntries(resolved.map((n) => [n.id, n]))

  useEffect(() => {
    fetch('/api/capabilities', { credentials: 'include' })
      .then(r => r.ok ? r.json() : null)
      .then(d => {
        if (!d?.capabilities) return
        setAvailable(new Set(
          (d.capabilities as { id: string; detected: boolean }[])
            .filter(c => c.detected)
            .map(c => c.id)
        ))
      })
      .catch(() => {})
  }, [])

  useEffect(() => {
    const apply = (name?: string) => { if (name) { setInstanceName(name); document.title = name } }
    fetch('/api/settings/public')
      .then(r => r.ok ? r.json() : null)
      .then((d: { instance_name?: string } | null) => apply(d?.instance_name))
      .catch(() => {})
    const handler = (e: Event) => apply((e as CustomEvent<{ instance_name: string }>).detail?.instance_name)
    window.addEventListener('vt-settings-changed', handler)
    return () => window.removeEventListener('vt-settings-changed', handler)
  }, [])

  const handleLogout = async () => {
    try {
      await api.auth.logout()
    } catch { /* ignore */ }
    logout()
    navigate('/login')
  }

  return (
    <aside
      style={{ width: collapsed ? 'var(--sidebar-collapsed-width)' : 'var(--sidebar-width)', background: 'var(--bg-panel)', borderColor: 'var(--border-subtle)' }}
      className="vt-sidebar flex flex-col h-full border-r transition-all duration-200"
    >
      {/* Logo */}
      <div className="flex items-center gap-2 px-3 py-4 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
        <Shield size={20} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
        {!collapsed && (
          <span className="font-semibold tracking-wide text-sm" style={{ color: 'var(--text-primary)' }}>
            {instanceName}
          </span>
        )}
        <button
          className="ml-auto md:hidden p-1 rounded"
          style={{ color: 'var(--text-muted)' }}
          onClick={() => document.body.classList.remove('mobile-nav-open')}
          aria-label="Close navigation"
        >
          <X size={16} />
        </button>
      </div>

      {/* Nav */}
      <nav className="flex-1 px-2 py-2 overflow-y-auto">
        {activeGroups.map((group, gi) => {
          const visibleItems = group.itemIds
            .map(id => NAV_ITEMS_BY_ID[id])
            .filter((item): item is NavItem => {
              if (!item) return false
              if (item.requires && available !== null && !available.has(item.requires)) return false
              const cfg = navMap[item.to.replace(/^\//, '')]
              if (cfg && !cfg.visible) return false
              return true
            })
          if (visibleItems.length === 0) return null
          return (
            <div key={group.id} className={gi > 0 ? 'mt-3' : ''}>
              {!collapsed && (
                <div
                  className="px-2 mb-1 text-xs font-medium uppercase tracking-widest select-none"
                  style={{ color: 'var(--text-disabled)', letterSpacing: '0.08em' }}
                >
                  {group.label}
                </div>
              )}
              {collapsed && gi > 0 && (
                <div className="mx-2 mb-2 mt-1" style={{ height: 1, background: 'var(--border-subtle)' }} />
              )}
              <div className="space-y-0.5">
                {visibleItems.map(({ to, icon: Icon, label }) => {
                  const key = to.replace(/^\//, '')
                  const displayLabel = navMap[key]?.label ?? label
                  return (
                    <NavLink
                      key={to}
                      to={to}
                      onClick={() => document.body.classList.remove('mobile-nav-open')}
                      className={({ isActive }) =>
                        clsx(
                          'flex items-center gap-3 px-2 py-2 rounded text-sm transition-colors',
                          isActive ? 'font-medium' : 'hover:opacity-80',
                        )
                      }
                      style={({ isActive }) => ({
                        background: isActive ? 'var(--accent-primary-subtle)' : 'transparent',
                        color: isActive ? 'var(--accent-primary)' : 'var(--text-secondary)',
                      })}
                      title={collapsed ? displayLabel : undefined}
                    >
                      <Icon size={16} style={{ flexShrink: 0 }} />
                      {!collapsed && <span>{displayLabel}</span>}
                    </NavLink>
                  )
                })}
              </div>
            </div>
          )
        })}
      </nav>

      {/* Footer */}
      <div className="border-t px-2 py-3 space-y-1" style={{ borderColor: 'var(--border-subtle)' }}>
        {!collapsed && user && (
          <div className="px-2 py-1 text-xs truncate" style={{ color: 'var(--text-muted)' }}>
            {user.username} · {user.role}
          </div>
        )}
        <button
          onClick={handleLogout}
          className="flex items-center gap-3 w-full px-2 py-2 rounded text-sm transition-colors hover:opacity-80"
          style={{ color: 'var(--text-secondary)' }}
          title={collapsed ? 'Logout' : undefined}
        >
          <LogOut size={16} style={{ flexShrink: 0 }} />
          {!collapsed && <span>Logout</span>}
        </button>
        <button
          onClick={() => setCollapsed((c) => !c)}
          className="flex items-center gap-3 w-full px-2 py-2 rounded text-sm transition-colors hover:opacity-80"
          style={{ color: 'var(--text-muted)' }}
        >
          {collapsed ? <ChevronRight size={16} /> : (
            <>
              <ChevronLeft size={16} />
              <span>Collapse</span>
            </>
          )}
        </button>
      </div>
    </aside>
  )
}
