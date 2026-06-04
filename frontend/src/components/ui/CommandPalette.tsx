import { Command } from 'cmdk'
import { useNavigate } from 'react-router-dom'
import {
  LayoutDashboard, Server, Container, Package, Bell,
  HardDrive, Terminal, ClipboardList, Settings, LogOut,
  Network, Shield,
} from 'lucide-react'
import { useCmdPaletteStore } from '@/store/cmdpalette'
import { useAuthStore } from '@/store/auth'
import { api } from '@/api/client'

const NAV_COMMANDS = [
  { id: 'dashboard',  label: 'Go to Dashboard',  icon: LayoutDashboard, to: '/dashboard'  },
  { id: 'services',   label: 'Go to Services',    icon: Server,          to: '/services'   },
  { id: 'containers', label: 'Go to Containers',  icon: Container,       to: '/containers' },
  { id: 'apps',       label: 'Go to App Vault',   icon: Package,         to: '/apps'       },
  { id: 'alerts',     label: 'Go to Alerts',      icon: Bell,            to: '/alerts'     },
  { id: 'backups',    label: 'Go to Backups',      icon: HardDrive,      to: '/backups'    },
  { id: 'storage',    label: 'Go to Storage',      icon: HardDrive,      to: '/storage'    },
  { id: 'network',    label: 'Go to Network',      icon: Network,        to: '/network'    },
  { id: 'security',   label: 'Go to Security',     icon: Shield,         to: '/security'   },
  { id: 'terminal',   label: 'Open Terminal',      icon: Terminal,       to: '/terminal'   },
  { id: 'audit',      label: 'Go to Audit Log',    icon: ClipboardList,  to: '/audit'      },
  { id: 'settings',   label: 'Go to Settings',     icon: Settings,       to: '/settings'   },
]

export default function CommandPalette() {
  const { open, setOpen, query, setQuery } = useCmdPaletteStore()
  const navigate = useNavigate()
  const logout = useAuthStore((s) => s.logout)

  if (!open) return null

  const run = (to?: string, action?: () => void) => {
    setOpen(false)
    setQuery('')
    if (to) navigate(to)
    if (action) action()
  }

  const handleLogout = async () => {
    try { await api.auth.logout() } catch {}
    logout()
    navigate('/login')
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-24"
      style={{ background: 'rgba(0,0,0,0.6)' }}
      onClick={() => setOpen(false)}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-md rounded-lg shadow-2xl overflow-hidden"
        style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)' }}
      >
        <Command>
          <div style={{ borderBottom: '1px solid var(--border-subtle)' }}>
            <Command.Input
              value={query}
              onValueChange={setQuery}
              placeholder="Search pages and actions…"
              className="w-full px-4 py-3 text-sm outline-none bg-transparent"
              style={{ color: 'var(--text-primary)' }}
              autoFocus
            />
          </div>
          <Command.List className="max-h-80 overflow-auto py-2">
            <Command.Empty className="px-4 py-6 text-sm text-center" style={{ color: 'var(--text-muted)' }}>
              No results.
            </Command.Empty>
            <Command.Group heading="Navigate" className="px-2">
              {NAV_COMMANDS.map(({ id, label, icon: Icon, to }) => (
                <Command.Item
                  key={id}
                  value={label}
                  onSelect={() => run(to)}
                  className="flex items-center gap-3 px-3 py-2 rounded text-sm cursor-pointer"
                  style={{ color: 'var(--text-primary)' }}
                >
                  <Icon size={14} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
                  {label}
                </Command.Item>
              ))}
            </Command.Group>
            <Command.Group heading="Actions" className="px-2">
              <Command.Item
                value="logout sign out"
                onSelect={() => run(undefined, handleLogout)}
                className="flex items-center gap-3 px-3 py-2 rounded text-sm cursor-pointer"
                style={{ color: 'var(--accent-danger)' }}
              >
                <LogOut size={14} />
                Sign out
              </Command.Item>
            </Command.Group>
          </Command.List>
        </Command>
      </div>
    </div>
  )
}
