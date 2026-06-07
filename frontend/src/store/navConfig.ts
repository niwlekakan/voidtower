import { create } from 'zustand'
import { persist } from 'zustand/middleware'

export interface NavItem {
  id: string
  label: string
  visible: boolean
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
  { id: 'themes',       label: 'Themes',       visible: true },
  { id: 'timeline',     label: 'Timeline',     visible: true },
]

interface NavConfigState {
  items: NavItem[]
  setItems: (items: NavItem[]) => void
  resetItems: () => void
}

export const useNavConfigStore = create<NavConfigState>()(
  persist(
    (set) => ({
      items: [],
      setItems: (items) => set({ items }),
      resetItems: () => set({ items: [] }),
    }),
    { name: 'vt-nav-config' },
  ),
)

/**
 * Returns the effective ordered nav items, merging persisted config over defaults.
 * If items is empty (never configured), returns DEFAULT_NAV_ITEMS.
 */
export function resolvedNavItems(items: NavItem[]): NavItem[] {
  if (items.length === 0) return DEFAULT_NAV_ITEMS
  // Merge: add any defaults not present in stored list (e.g. newly added pages)
  const stored = new Set(items.map((i) => i.id))
  const extras = DEFAULT_NAV_ITEMS.filter((d) => !stored.has(d.id))
  return [...items, ...extras]
}
