import { useState } from 'react'
import { NavLink, useNavigate } from 'react-router-dom'
import {
  LayoutDashboard, Server, Container, Package, Bell,
  HardDrive, Network, Terminal, ClipboardList, Settings,
  ChevronLeft, ChevronRight, LogOut, Shield, Lock, BrainCircuit, FolderOpen, Globe, X, Cpu, Stethoscope, KeyRound, History, Flame, Zap, Wifi, Monitor, Tag, Palette, ArrowUpCircle, PlugZap,
} from 'lucide-react'
import { clsx } from 'clsx'
import { useAuthStore } from '@/store/auth'
import { api } from '@/api/client'

interface NavItem {
  to: string
  icon: React.ElementType
  label: string
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
    label: 'Infrastructure',
    items: [
      { to: '/services',   icon: Server,      label: 'Services'   },
      { to: '/containers', icon: Container,   label: 'Containers' },
      { to: '/vms',        icon: Monitor,     label: 'VMs'        },
      { to: '/apps',       icon: Package,     label: 'App Vault'  },
      { to: '/ai',         icon: BrainCircuit,label: 'AI'         },
      { to: '/models',     icon: HardDrive,   label: 'Models'     },
    ],
  },
  {
    label: 'Network',
    items: [
      { to: '/network',   icon: Network, label: 'Network'   },
      { to: '/proxies',   icon: Globe,   label: 'Proxies'   },
      { to: '/wireguard', icon: Wifi,    label: 'WireGuard' },
      { to: '/firewall',  icon: Flame,   label: 'Firewall'  },
    ],
  },
  {
    label: 'Storage',
    items: [
      { to: '/storage', icon: HardDrive,  label: 'Storage' },
      { to: '/backups', icon: HardDrive,  label: 'Backups' },
      { to: '/files',   icon: FolderOpen, label: 'Files'   },
    ],
  },
  {
    label: 'Security',
    items: [
      { to: '/security', icon: Lock,     label: 'Security'  },
      { to: '/secrets',  icon: KeyRound, label: 'Secrets'   },
      { to: '/audit',    icon: ClipboardList, label: 'Audit Log' },
    ],
  },
  {
    label: 'System',
    items: [
      { to: '/tags',        icon: Tag,         label: 'Tags'         },
      { to: '/automation',  icon: Zap,         label: 'Automation'   },
      { to: '/terminal',    icon: Terminal,    label: 'Terminal'     },
      { to: '/capabilities',icon: Cpu,         label: 'Capabilities' },
      { to: '/diagnostics', icon: Stethoscope, label: 'Diagnostics'  },
      { to: '/themes',      icon: Palette,     label: 'Themes'       },
      { to: '/updates',       icon: ArrowUpCircle, label: 'Updates'      },
      { to: '/integrations',  icon: PlugZap,       label: 'Integrations' },
      { to: '/settings',    icon: Settings,      label: 'Settings'     },
    ],
  },
]

export default function Sidebar() {
  const [collapsed, setCollapsed] = useState(false)
  const { user, logout } = useAuthStore()
  const navigate = useNavigate()

  const handleLogout = async () => {
    try {
      await api.auth.logout()
    } catch {}
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
            VoidTower
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
        {NAV_GROUPS.map((group, gi) => (
          <div key={group.label} className={gi > 0 ? 'mt-3' : ''}>
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
              {group.items.map(({ to, icon: Icon, label }) => (
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
                  title={collapsed ? label : undefined}
                >
                  <Icon size={16} style={{ flexShrink: 0 }} />
                  {!collapsed && <span>{label}</span>}
                </NavLink>
              ))}
            </div>
          </div>
        ))}
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
