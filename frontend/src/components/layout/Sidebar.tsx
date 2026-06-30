import { useState, useEffect, useRef } from 'react'
import { createPortal } from 'react-dom'
import { NavLink, useNavigate } from 'react-router-dom'
import {
  LayoutDashboard, Server, Container, Package, Bell,
  HardDrive, Network, Terminal, ClipboardList, Settings,
  ChevronLeft, ChevronDown, LogOut, Shield, Lock, BrainCircuit, FolderOpen, Globe, X, KeyRound, History, Flame, Zap, Wifi, Monitor, Tag, ArrowUpCircle, PlugZap, Puzzle, Palette, Blocks, Box, Wand2, LayoutPanelTop,
} from 'lucide-react'
import { clsx } from 'clsx'
import { useAuthStore } from '@/store/auth'
import { api } from '@/api/client'
import { useNavConfigStore, resolvedNavItems, resolvedNavGroups } from '@/store/navConfig'
import { useSidebarPrefsStore, type SidebarAnimationStyle } from '@/store/sidebarPrefs'
import { ICON_REGISTRY } from '@/components/ui/iconRegistry'
import { TopBarUtilities } from './TopBar'

export const MAIN_SCROLL_ID = 'vt-main-scroll'

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
      { to: '/lxc',        icon: Box,       label: 'LXC',       requires: 'pct'     },
      { to: '/proxmox',    icon: Server,    label: 'Proxmox'                        },
      { to: '/apps',       icon: Package,   label: 'App Vault'                      },
    ],
  },
  {
    label: 'AI',
    items: [
      { to: '/ai',      icon: BrainCircuit, label: 'Workspace' },
      { to: '/models',  icon: HardDrive,    label: 'Models'    },
      { to: '/studio',  icon: Wand2,        label: 'Studio'    },
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
      { to: '/policy',   icon: Shield,        label: 'Policy'    },
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
      { to: '/ai-providers',  icon: BrainCircuit,  label: 'AI Providers' },
      { to: '/integrations', icon: PlugZap,       label: 'Integrations' },
      { to: '/updates',      icon: ArrowUpCircle, label: 'Updates'      },
      { to: '/mods',         icon: Puzzle,        label: 'Mods'         },
      { to: '/plugins',      icon: Blocks,        label: 'Plugins'      },
      { to: '/customization', icon: Palette,       label: 'Customization' },
      { to: '/settings',     icon: Settings,      label: 'Settings'     },
    ],
  },
]

// Flat lookup: id -> NavItem definition (icon, route, requires)
const NAV_ITEMS_BY_ID: Record<string, NavItem> = Object.fromEntries(
  NAV_GROUPS.flatMap(g => g.items).map(item => [item.to.replace(/^\//, ''), item])
)

const EASE_STD    = 'cubic-bezier(0.4, 0, 0.2, 1)'
const EASE_SPRING = 'cubic-bezier(0.34, 1.56, 0.64, 1)'

function asideTransition(animation: SidebarAnimationStyle): string {
  if (animation === 'squeeze') return `width 420ms ${EASE_SPRING}, box-shadow 280ms ease`
  if (animation === 'bounce') return `width 460ms ${EASE_SPRING}, box-shadow 280ms ease`
  if (animation === 'fade') return `width 220ms ease, box-shadow 220ms ease`
  if (animation === 'flip') return `width 320ms ${EASE_STD}, box-shadow 280ms ease`
  return `width 280ms ${EASE_STD}, box-shadow 280ms ease`
}

function labelStyle(collapsed: boolean, animation: SidebarAnimationStyle, index = 0): React.CSSProperties {
  const base: React.CSSProperties = {
    overflow: 'hidden',
    whiteSpace: 'nowrap',
    display: 'inline-block',
    maxWidth: collapsed ? 0 : 160,
    opacity: collapsed ? 0 : 1,
  }

  switch (animation) {
    case 'fade':
      return {
        ...base,
        transition: collapsed
          ? `opacity 110ms ease, max-width 200ms ${EASE_STD}`
          : `opacity 200ms ease, max-width 240ms ${EASE_STD}`,
      }
    case 'squeeze':
      return {
        ...base,
        transform: `scale(${collapsed ? 0.85 : 1})`,
        transformOrigin: 'left center',
        transition: collapsed
          ? `opacity 120ms ease, max-width 220ms ${EASE_STD}, transform 200ms ease`
          : `opacity 220ms ease 80ms, max-width 340ms ${EASE_SPRING}, transform 340ms ${EASE_SPRING} 80ms`,
      }
    case 'stagger': {
      const delay = collapsed ? (7 - index) * 18 : index * 28
      return {
        ...base,
        transform: `translateX(${collapsed ? -8 : 0}px)`,
        transition: collapsed
          ? `opacity 110ms ease ${Math.max(delay, 0)}ms, max-width 200ms ${EASE_STD}, transform 160ms ease ${Math.max(delay, 0)}ms`
          : `opacity 220ms ease ${60 + delay}ms, max-width 260ms ${EASE_STD}, transform 260ms ${EASE_STD} ${60 + delay}ms`,
      }
    }
    case 'flip':
      return {
        ...base,
        transformOrigin: 'left center',
        transform: `perspective(400px) rotateY(${collapsed ? -90 : 0}deg)`,
        transition: collapsed
          ? `opacity 130ms ease, max-width 220ms ${EASE_STD}, transform 220ms ${EASE_STD}`
          : `opacity 200ms ease 90ms, max-width 260ms ${EASE_STD}, transform 320ms ${EASE_SPRING} 60ms`,
      }
    case 'bounce':
      return {
        ...base,
        transform: `translateY(${collapsed ? -8 : 0}px)`,
        transition: collapsed
          ? `opacity 120ms ease, max-width 200ms ${EASE_STD}, transform 180ms ease`
          : `opacity 220ms ease 60ms, max-width 260ms ${EASE_STD}, transform 380ms ${EASE_SPRING} 60ms`,
      }
    case 'slide':
    default:
      return {
        ...base,
        transform: `translateX(${collapsed ? -5 : 0}px)`,
        transition: collapsed
          ? `opacity 130ms ease, max-width 220ms ${EASE_STD}, transform 200ms ease`
          : `opacity 200ms ease 70ms, max-width 260ms ${EASE_STD}, transform 230ms ease 70ms`,
      }
  }
}

function groupHeaderStyle(collapsed: boolean, animation: SidebarAnimationStyle): React.CSSProperties {
  const base: React.CSSProperties = {
    color: 'var(--text-disabled)',
    letterSpacing: '0.08em',
    overflow: 'hidden',
    maxHeight: collapsed ? 0 : 24,
    opacity: collapsed ? 0 : 1,
    marginBottom: collapsed ? 0 : 4,
  }
  if (animation === 'fade') {
    return {
      ...base,
      transition: collapsed
        ? `opacity 100ms ease, max-height 200ms ${EASE_STD}, margin-bottom 180ms ease`
        : `opacity 180ms ease, max-height 240ms ${EASE_STD}, margin-bottom 240ms ease`,
    }
  }
  if (animation === 'squeeze') {
    return {
      ...base,
      transition: collapsed
        ? `opacity 120ms ease, max-height 240ms ${EASE_STD}, margin-bottom 200ms ease`
        : `opacity 220ms ease 80ms, max-height 360ms ${EASE_SPRING}, margin-bottom 360ms ${EASE_SPRING} 80ms`,
    }
  }
  if (animation === 'flip') {
    return {
      ...base,
      transformOrigin: 'top',
      transform: `rotateX(${collapsed ? -90 : 0}deg)`,
      transition: collapsed
        ? `opacity 100ms ease, max-height 200ms ${EASE_STD}, margin-bottom 180ms ease, transform 200ms ${EASE_STD}`
        : `opacity 180ms ease 80ms, max-height 240ms ${EASE_STD}, margin-bottom 240ms ease, transform 280ms ${EASE_SPRING} 60ms`,
    }
  }
  if (animation === 'bounce') {
    return {
      ...base,
      transform: `translateY(${collapsed ? -6 : 0}px)`,
      transition: collapsed
        ? `opacity 100ms ease, max-height 200ms ${EASE_STD}, margin-bottom 180ms ease, transform 160ms ease`
        : `opacity 200ms ease 60ms, max-height 280ms ${EASE_STD}, margin-bottom 280ms ease, transform 380ms ${EASE_SPRING} 60ms`,
    }
  }
  return {
    ...base,
    transition: collapsed
      ? `opacity 120ms ease, max-height 240ms ${EASE_STD}, margin-bottom 200ms ease`
      : `opacity 200ms ease 50ms, max-height 280ms ${EASE_STD}, margin-bottom 280ms ease 50ms`,
  }
}

function dividerStyle(collapsed: boolean): React.CSSProperties {
  return {
    height: 1,
    background: 'var(--border-subtle)',
    opacity: collapsed ? 1 : 0,
    marginBottom: collapsed ? 8 : 0,
    marginTop: collapsed ? 4 : 0,
    transition: `opacity 200ms ease, margin-bottom 250ms ease, margin-top 250ms ease`,
  }
}

function chevronStyle(collapsed: boolean, animation: SidebarAnimationStyle): React.CSSProperties {
  if (animation === 'fade') {
    return {
      flexShrink: 0,
      transform: `rotate(${collapsed ? 180 : 0}deg)`,
      transition: `transform 220ms ease`,
    }
  }
  if (animation === 'squeeze') {
    return {
      flexShrink: 0,
      transform: `rotate(${collapsed ? 180 : 0}deg) scale(${collapsed ? 0.85 : 1})`,
      transition: `transform 360ms ${EASE_SPRING}`,
    }
  }
  if (animation === 'flip') {
    return {
      flexShrink: 0,
      transform: `perspective(200px) rotateY(${collapsed ? 180 : 0}deg)`,
      transition: `transform 320ms ${EASE_SPRING}`,
    }
  }
  if (animation === 'bounce') {
    return {
      flexShrink: 0,
      transform: `rotate(${collapsed ? 180 : 0}deg)`,
      transition: `transform 420ms ${EASE_SPRING}`,
    }
  }
  return {
    flexShrink: 0,
    transform: `rotate(${collapsed ? 180 : 0}deg)`,
    transition: `transform 300ms ${EASE_SPRING}`,
  }
}

export default function Sidebar() {
  const [collapsed, setCollapsed] = useState(false)
  const [scrollHidden, setScrollHidden] = useState(false)
  const [openGroup, setOpenGroup] = useState<{ id: string; left: number; top?: number; bottom?: number } | null>(null)
  const [available, setAvailable] = useState<Set<string> | null>(null)
  const [instanceName, setInstanceName] = useState('VoidTower')
  const [instanceLogo, setInstanceLogo] = useState('')
  const [activePlugins, setActivePlugins] = useState<{ id: string; name: string }[]>([])
  const [customTabs, setCustomTabs] = useState<{ id: string; title: string; icon: string | null }[]>([])
  const { user, logout } = useAuthStore()
  const navigate = useNavigate()
  const navItems = useNavConfigStore((s) => s.items)
  const storedGroups = useNavConfigStore((s) => s.navGroups)
  const resolved = resolvedNavItems(navItems)
  const activeGroups = resolvedNavGroups(storedGroups)
  const navMap = Object.fromEntries(resolved.map((n) => [n.id, n]))
  const animation = useSidebarPrefsStore((s) => s.animation)
  const placement = useSidebarPrefsStore((s) => s.placement)
  const autoHide = useSidebarPrefsStore((s) => s.autoHide)
  const horizontal = placement === 'top' || placement === 'bottom'
  const effectiveCollapsed = horizontal ? false : (collapsed || (autoHide && scrollHidden))

  // Placement changes don't remount this component (AppLayout keeps it in a stable JSX
  // slot, repositioned via CSS order) — but the horizontal/vertical branches below are
  // structurally different <aside> trees, so there's no single box to morph smoothly.
  // Instead, slide+fade the new shape in from the edge it's docking to.
  const [entering, setEntering] = useState(false)
  const prevPlacementRef = useRef(placement)
  useEffect(() => {
    if (prevPlacementRef.current === placement) return
    prevPlacementRef.current = placement
    setEntering(true)
    let raf2 = 0
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => setEntering(false))
    })
    return () => { cancelAnimationFrame(raf1); cancelAnimationFrame(raf2) }
  }, [placement])
  const enterTransform =
    placement === 'left' ? 'translateX(-28px)' :
    placement === 'right' ? 'translateX(28px)' :
    placement === 'top' ? 'translateY(-28px)' :
    'translateY(28px)'

  useEffect(() => {
    if (!autoHide) { setScrollHidden(false); return }
    const el = document.getElementById(MAIN_SCROLL_ID)
    if (!el) return
    let lastTop = el.scrollTop
    const handler = () => {
      const top = el.scrollTop
      if (top > lastTop && top > 40) setScrollHidden(true)
      else if (top < lastTop) setScrollHidden(false)
      lastTop = top
    }
    el.addEventListener('scroll', handler, { passive: true })
    return () => el.removeEventListener('scroll', handler)
  }, [autoHide])

  useEffect(() => {
    fetch('/api/plugins', { credentials: 'include' })
      .then(r => r.ok ? r.json() : [])
      .then((data: { id: string; name: string; enabled: boolean }[]) =>
        setActivePlugins(data.filter(p => p.enabled).map(p => ({ id: p.id, name: p.name })))
      )
      .catch(() => {})
  }, [])

  useEffect(() => {
    api.tabs.list()
      .then(tabs => setCustomTabs(tabs.map(t => ({ id: t.id, title: t.title, icon: t.icon }))))
      .catch(() => {})
  }, [])

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
    const apply = (name?: string, logo?: string) => {
      if (name) { setInstanceName(name); document.title = name }
      if (logo !== undefined) setInstanceLogo(logo)
    }
    fetch('/api/settings/public')
      .then(r => r.ok ? r.json() : null)
      .then((d: { instance_name?: string; instance_logo?: string } | null) => apply(d?.instance_name, d?.instance_logo))
      .catch(() => {})
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ instance_name?: string; instance_logo?: string }>).detail
      apply(detail?.instance_name, detail?.instance_logo)
    }
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

  // Flatten + filter once, shared by both horizontal and vertical rendering
  const visibleGroups = activeGroups
    .map((group) => ({
      group,
      items: group.itemIds
        .map(id => NAV_ITEMS_BY_ID[id])
        .filter((item): item is NavItem => {
          if (!item) return false
          if (item.requires && available !== null && !available.has(item.requires)) return false
          const cfg = navMap[item.to.replace(/^\//, '')]
          if (cfg && !cfg.visible) return false
          return true
        }),
    }))
    .filter((g) => g.items.length > 0)

  const renderDropdownGroup = (id: string, label: string, items: { to: string; icon: React.ElementType; label: string }[]) => {
    const isOpen = openGroup?.id === id
    const toggle = (e: React.MouseEvent<HTMLButtonElement>) => {
      if (isOpen) { setOpenGroup(null); return }
      const r = e.currentTarget.getBoundingClientRect()
      if (placement === 'bottom') {
        setOpenGroup({ id, left: r.left, bottom: window.innerHeight - r.top + 4 })
      } else {
        setOpenGroup({ id, left: r.left, top: r.bottom + 4 })
      }
    }
    return (
      <div key={id} style={{ flexShrink: 0 }}>
        <button
          onClick={toggle}
          className="flex items-center gap-1 px-2 py-1.5 rounded text-sm whitespace-nowrap transition-colors hover:opacity-80"
          style={{ color: 'var(--text-secondary)', background: isOpen ? 'var(--bg-elevated)' : 'transparent' }}
        >
          <span>{label}</span>
          <ChevronDown size={12} style={{ transform: `rotate(${isOpen ? 180 : 0}deg)`, transition: 'transform 160ms ease', flexShrink: 0 }} />
        </button>
        {isOpen && openGroup && createPortal(
          <>
            <div onClick={() => setOpenGroup(null)} style={{ position: 'fixed', inset: 0, zIndex: 9998 }} />
            <div
              style={{
                position: 'fixed', left: openGroup.left, top: openGroup.top, bottom: openGroup.bottom, zIndex: 9999,
                minWidth: 180, padding: 4, borderRadius: 6,
                background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)',
                boxShadow: '0 8px 20px rgba(0,0,0,0.25)',
              }}
            >
              {items.map(({ to, icon: DefaultIcon, label: itemLabel }) => {
                const key = to.replace(/^\//, '')
                const cfg = navMap[key]
                const displayLabel = cfg?.label ?? itemLabel
                const Icon = (cfg?.icon && ICON_REGISTRY[cfg.icon]) || DefaultIcon
                return (
                  <NavLink
                    key={to}
                    to={to}
                    onClick={() => setOpenGroup(null)}
                    className={({ isActive }) =>
                      clsx('flex items-center gap-2 px-2 py-1.5 rounded text-sm whitespace-nowrap transition-colors', isActive ? 'font-medium' : 'hover:opacity-80')
                    }
                    style={({ isActive }) => ({
                      background: isActive ? 'var(--accent-primary-subtle)' : 'transparent',
                      color: isActive ? 'var(--accent-primary)' : 'var(--text-secondary)',
                    })}
                  >
                    <Icon size={15} style={{ flexShrink: 0 }} />
                    <span>{displayLabel}</span>
                  </NavLink>
                )
              })}
            </div>
          </>,
          document.body,
        )}
      </div>
    )
  }

  if (horizontal) {
    const isTop = placement === 'top'
    return (
      <aside
        style={{
          width: '100%',
          height: 56,
          background: 'var(--bg-panel)',
          borderColor: 'var(--border-subtle)',
          boxShadow: scrollHidden ? 'none' : `0 ${isTop ? '2px' : '-2px'} 12px rgba(0,0,0,0.18)`,
          transform: entering ? enterTransform : `translateY(${scrollHidden ? (isTop ? '-100%' : '100%') : '0'})`,
          opacity: entering ? 0 : 1,
          transition: entering ? 'none' : `transform 280ms ${EASE_STD}, opacity 220ms ease, box-shadow 220ms ease`,
          pointerEvents: scrollHidden ? 'none' : 'auto',
        }}
        className={clsx('vt-sidebar flex items-center px-3 gap-3 flex-shrink-0', isTop ? 'border-b' : 'border-t')}
      >
        {instanceLogo
          ? <img src={instanceLogo} alt="" style={{ width: 20, height: 20, objectFit: 'contain', flexShrink: 0 }} />
          : <Shield size={20} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
        }
        <span className="font-semibold tracking-wide text-sm flex-shrink-0" style={{ color: 'var(--text-primary)' }}>
          {instanceName}
        </span>
        <div className="flex items-center gap-1 flex-1 min-w-0 overflow-x-auto">
          {visibleGroups.map(({ group, items }) => renderDropdownGroup(group.id, group.label, items))}
          {activePlugins.length > 0 &&
            renderDropdownGroup('plugins', 'Plugins', activePlugins.map((p) => ({ to: `/plugins/${p.id}`, icon: Blocks, label: p.name })))}
          {customTabs.length > 0 &&
            renderDropdownGroup('my-tabs', 'My Tabs', customTabs.map((t) => ({ to: `/tabs/${t.id}`, icon: (t.icon && ICON_REGISTRY[t.icon]) || LayoutPanelTop, label: t.title })))}
        </div>
        {/* Search/GPU/UI-mode/tag/status/bell — merged in here instead of a separate TopBar row */}
        <TopBarUtilities compact />
        <button
          onClick={handleLogout}
          className="flex items-center gap-1.5 px-2 py-1.5 rounded text-sm flex-shrink-0 transition-colors hover:opacity-80"
          style={{ color: 'var(--text-secondary)' }}
          title="Logout"
        >
          <LogOut size={16} />
        </button>
      </aside>
    )
  }

  const onRight = placement === 'right'

  return (
    <aside
      style={{
        width: effectiveCollapsed ? 'var(--sidebar-collapsed-width)' : 'var(--sidebar-width)',
        background: 'var(--bg-panel)',
        borderColor: 'var(--border-subtle)',
        boxShadow: effectiveCollapsed ? 'none' : `${onRight ? '-2px' : '2px'} 0 12px rgba(0,0,0,0.18)`,
        transform: entering ? enterTransform : 'none',
        opacity: entering ? 0 : 1,
        transition: entering ? 'none' : `${asideTransition(animation)}, transform 260ms ${EASE_STD}, opacity 220ms ease`,
      }}
      className={clsx('vt-sidebar flex flex-col h-full', onRight ? 'border-l' : 'border-r')}
    >
      {/* Logo */}
      <div className="flex items-center gap-2 px-3 py-4 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
        {instanceLogo
          ? <img src={instanceLogo} alt="" style={{ width: 20, height: 20, objectFit: 'contain', flexShrink: 0 }} />
          : <Shield size={20} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
        }
        <span
          className="font-semibold tracking-wide text-sm"
          style={{
            color: 'var(--text-primary)',
            overflow: 'hidden',
            whiteSpace: 'nowrap',
            display: 'inline-block',
            maxWidth: effectiveCollapsed ? 0 : 160,
            opacity: effectiveCollapsed ? 0 : 1,
            transform: `translateX(${effectiveCollapsed ? -6 : 0}px)`,
            transition: effectiveCollapsed
              ? `opacity 150ms ease, max-width 250ms ${EASE_STD}, transform 200ms ease`
              : `opacity 220ms ease 60ms, max-width 280ms ${EASE_STD}, transform 250ms ease 60ms`,
          }}
        >
          {instanceName}
        </span>
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
        {(() => { let staggerIndex = -1; return visibleGroups.map(({ group, items: visibleItems }, gi) => {
          return (
            <div key={group.id} className={gi > 0 ? 'mt-3' : ''}>
              <div
                className="px-2 text-xs font-medium uppercase tracking-widest select-none"
                style={groupHeaderStyle(effectiveCollapsed, animation)}
              >
                {group.label}
              </div>
              {gi > 0 && <div className="mx-2" style={dividerStyle(effectiveCollapsed)} />}
              <div className="space-y-0.5">
                {visibleItems.map(({ to, icon: DefaultIcon, label }) => {
                  const key = to.replace(/^\//, '')
                  const cfg = navMap[key]
                  const displayLabel = cfg?.label ?? label
                  const Icon = (cfg?.icon && ICON_REGISTRY[cfg.icon]) || DefaultIcon
                  staggerIndex += 1
                  const itemIndex = staggerIndex
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
                      title={effectiveCollapsed ? displayLabel : undefined}
                    >
                      <Icon size={16} style={{ flexShrink: 0 }} />
                      <span style={labelStyle(effectiveCollapsed, animation, itemIndex)}>{displayLabel}</span>
                    </NavLink>
                  )
                })}
              </div>
            </div>
          )
        }) })()}
          {/* Dynamic plugin pages */}
          {activePlugins.length > 0 && (
            <div className="mt-3">
              <div
                className="px-2 text-xs font-medium uppercase tracking-widest select-none"
                style={groupHeaderStyle(effectiveCollapsed, animation)}
              >
                Plugins
              </div>
              <div className="mx-2" style={dividerStyle(effectiveCollapsed)} />
              <div className="space-y-0.5">
                {activePlugins.map((p, pi) => (
                  <NavLink
                    key={p.id}
                    to={`/plugins/${p.id}`}
                    onClick={() => document.body.classList.remove('mobile-nav-open')}
                    className={({ isActive }) =>
                      clsx('flex items-center gap-3 px-2 py-2 rounded text-sm transition-colors', isActive ? 'font-medium' : 'hover:opacity-80')
                    }
                    style={({ isActive }) => ({
                      background: isActive ? 'var(--accent-primary-subtle)' : 'transparent',
                      color: isActive ? 'var(--accent-primary)' : 'var(--text-secondary)',
                    })}
                    title={effectiveCollapsed ? p.name : undefined}
                  >
                    <Blocks size={16} style={{ flexShrink: 0 }} />
                    <span style={labelStyle(effectiveCollapsed, animation, pi)}>{p.name}</span>
                  </NavLink>
                ))}
              </div>
            </div>
          )}
          {/* Personal custom tabs */}
          {customTabs.length > 0 && (
            <div className="mt-3">
              <div
                className="px-2 text-xs font-medium uppercase tracking-widest select-none"
                style={groupHeaderStyle(effectiveCollapsed, animation)}
              >
                My Tabs
              </div>
              <div className="mx-2" style={dividerStyle(effectiveCollapsed)} />
              <div className="space-y-0.5">
                {customTabs.map((t, ti) => {
                  const Icon = (t.icon && ICON_REGISTRY[t.icon]) || LayoutPanelTop
                  return (
                    <NavLink
                      key={t.id}
                      to={`/tabs/${t.id}`}
                      onClick={() => document.body.classList.remove('mobile-nav-open')}
                      className={({ isActive }) =>
                        clsx('flex items-center gap-3 px-2 py-2 rounded text-sm transition-colors', isActive ? 'font-medium' : 'hover:opacity-80')
                      }
                      style={({ isActive }) => ({
                        background: isActive ? 'var(--accent-primary-subtle)' : 'transparent',
                        color: isActive ? 'var(--accent-primary)' : 'var(--text-secondary)',
                      })}
                      title={effectiveCollapsed ? t.title : undefined}
                    >
                      <Icon size={16} style={{ flexShrink: 0 }} />
                      <span style={labelStyle(effectiveCollapsed, animation, ti)}>{t.title}</span>
                    </NavLink>
                  )
                })}
              </div>
            </div>
          )}
      </nav>

      {/* Footer */}
      <div className="border-t px-2 py-3 space-y-1" style={{ borderColor: 'var(--border-subtle)' }}>
        {!effectiveCollapsed && user && (
          <div className="px-2 py-1 text-xs truncate" style={{ color: 'var(--text-muted)' }}>
            {user.username} · {user.role}
          </div>
        )}
        <button
          onClick={handleLogout}
          className="flex items-center gap-3 w-full px-2 py-2 rounded text-sm transition-colors hover:opacity-80"
          style={{ color: 'var(--text-secondary)' }}
          title={effectiveCollapsed ? 'Logout' : undefined}
        >
          <LogOut size={16} style={{ flexShrink: 0 }} />
          <span style={labelStyle(effectiveCollapsed, animation)}>Logout</span>
        </button>
        <button
          onClick={() => setCollapsed((c) => !c)}
          className="flex items-center gap-3 w-full px-2 py-2 rounded text-sm transition-colors hover:opacity-80"
          style={{ color: 'var(--text-muted)' }}
        >
          <ChevronLeft size={16} style={chevronStyle(effectiveCollapsed, animation)} />
          <span style={labelStyle(effectiveCollapsed, animation)}>Collapse</span>
        </button>
      </div>
    </aside>
  )
}
