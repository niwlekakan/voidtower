import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export interface NavItem {
  id: string
  label: string
  visible: boolean
  /** Lucide icon name from ICON_REGISTRY — overrides the item's default icon when set */
  icon?: string
}

export interface StoredNavGroup {
  id: string
  label: string
  itemIds: string[]
}

// Default ordered list derived from DOCK_ITEMS in AiosDock.tsx
export const DEFAULT_NAV_ITEMS: NavItem[] = [
  { id: 'dashboard',    label: 'Dashboard',    visible: true },
  { id: 'odysseus',     label: 'Odysseus',     visible: true },
  { id: 'terminal',     label: 'Terminal',     visible: true },
  { id: 'apps',         label: 'App Vault',    visible: true },
  { id: 'containers',   label: 'Containers',   visible: true },
  { id: 'vms',          label: 'VMs',          visible: true },
  { id: 'proxmox',      label: 'Proxmox',      visible: true },
  { id: 'files',        label: 'Files',        visible: true },
  { id: 'alerts',       label: 'Alerts',       visible: true },
  { id: 'settings',     label: 'Settings',     visible: true },
  { id: 'services',     label: 'Services',     visible: true },
  { id: 'ai',           label: 'AI',           visible: true },
  { id: 'models',       label: 'Models',       visible: true },
  { id: 'network',      label: 'Network',      visible: true },
  { id: 'proxies',      label: 'Proxies',      visible: true },
  { id: 'wireguard',    label: 'WireGuard',    visible: true },
  { id: 'firewall',     label: 'Firewall',     visible: true },
  { id: 'storage',      label: 'Storage',      visible: true },
  { id: 'backups',      label: 'Backups',      visible: true },
  { id: 'security',     label: 'Security',     visible: true },
  { id: 'secrets',      label: 'Secrets',      visible: true },
  { id: 'audit',        label: 'Audit Log',    visible: true },
  { id: 'automation',   label: 'Automation',   visible: true },
  { id: 'tags',         label: 'Tags',         visible: true },
  { id: 'integrations', label: 'Integrations', visible: true },
  { id: 'updates',      label: 'Updates',      visible: true },
  { id: 'mods',         label: 'Mods',         visible: true },
  { id: 'capabilities', label: 'Capabilities', visible: true },
  { id: 'diagnostics',  label: 'Diagnostics',  visible: true },
  { id: 'customization', label: 'Customization', visible: true },
  { id: 'timeline',     label: 'Timeline',     visible: true },
]

export const DEFAULT_NAV_GROUPS: StoredNavGroup[] = [
  { id: 'overview',  label: 'Overview',  itemIds: ['dashboard', 'alerts', 'timeline'] },
  { id: 'resources', label: 'Resources', itemIds: ['services', 'containers', 'vms', 'proxmox', 'apps'] },
  { id: 'ai',        label: 'AI',        itemIds: ['ai', 'models'] },
  { id: 'network',   label: 'Network',   itemIds: ['network', 'proxies', 'wireguard', 'firewall'] },
  { id: 'data',      label: 'Data',      itemIds: ['storage', 'backups', 'files'] },
  { id: 'security',  label: 'Security',  itemIds: ['security', 'secrets', 'audit'] },
  { id: 'ops',       label: 'Ops',       itemIds: ['automation', 'terminal', 'tags'] },
  { id: 'system',    label: 'System',    itemIds: ['integrations', 'updates', 'mods', 'customization', 'settings'] },
]

async function pushToServer(items: NavItem[], navGroups: StoredNavGroup[]) {
  try {
    await fetch('/api/nav-config', {
      method: 'POST',
      credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ items, nav_groups: navGroups }),
    })
  } catch {
    // best-effort sync — localStorage remains the source of truth on this device if it fails
  }
}

interface NavConfigState {
  items: NavItem[]
  setItems: (items: NavItem[]) => void
  resetItems: () => void
  navGroups: StoredNavGroup[]
  setNavGroups: (groups: StoredNavGroup[]) => void
  resetNavGroups: () => void
  hydrated: boolean
  hydrateFromServer: () => Promise<void>
}

export const useNavConfigStore = create<NavConfigState>()(
  persist(
    (set, get) => ({
      items: [],
      setItems: (items) => { set({ items }); void pushToServer(items, get().navGroups) },
      resetItems: () => { set({ items: [] }); void pushToServer([], get().navGroups) },
      navGroups: [],
      setNavGroups: (navGroups) => { set({ navGroups }); void pushToServer(get().items, navGroups) },
      resetNavGroups: () => { set({ navGroups: [] }); void pushToServer(get().items, []) },
      hydrated: false,
      hydrateFromServer: async () => {
        if (get().hydrated) return
        set({ hydrated: true })
        try {
          const res = await fetch('/api/nav-config', { credentials: 'include' })
          if (!res.ok) return
          const data: { items: NavItem[] | null; nav_groups: StoredNavGroup[] | null; source: string } = await res.json()
          if (data.source === 'user' && Array.isArray(data.items)) {
            set({ items: data.items, navGroups: Array.isArray(data.nav_groups) ? data.nav_groups : [] })
            return
          }
          // No per-user config saved yet. Migrate any existing local customization up,
          // otherwise fall back to the owner-set instance default (if any).
          const local = get()
          if (local.items.length > 0 || local.navGroups.length > 0) {
            void pushToServer(local.items, local.navGroups)
            return
          }
          const defRes = await fetch('/api/nav-config/default', { credentials: 'include' })
          if (!defRes.ok) return
          const def: { items?: NavItem[]; nav_groups?: StoredNavGroup[] } | null = await defRes.json()
          if (def && Array.isArray(def.items)) {
            set({ items: def.items, navGroups: Array.isArray(def.nav_groups) ? def.nav_groups : [] })
          }
        } catch {
          // offline / backend unreachable — keep whatever localStorage already has
        }
      },
    }),
    {
      name: 'vt-nav-config',
      partialize: (state) => ({ items: state.items, navGroups: state.navGroups }),
    },
  ),
)

/**
 * Returns effective ordered nav items, merging persisted config over defaults.
 * If items is empty (never configured), returns DEFAULT_NAV_ITEMS.
 */
export function resolvedNavItems(items: NavItem[]): NavItem[] {
  if (items.length === 0) return DEFAULT_NAV_ITEMS
  const stored = new Set(items.map((i) => i.id))
  const extras = DEFAULT_NAV_ITEMS.filter((d) => !stored.has(d.id))
  return [...items, ...extras]
}

/**
 * Returns effective nav groups. If navGroups is empty (never configured), returns DEFAULT_NAV_GROUPS.
 */
export function resolvedNavGroups(navGroups: StoredNavGroup[]): StoredNavGroup[] {
  return navGroups.length > 0 ? navGroups : DEFAULT_NAV_GROUPS
}
