import { create } from 'zustand'
import { persist, createJSONStorage } from 'zustand/middleware'

export type LayoutMode =
  | 'floating' | 'left-half' | 'right-half' | 'top-half' | 'bottom-half'
  | 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right'
  | 'fullscreen' | 'minimized' | 'sheet' | 'tile'

export type PanelType = 'app' | 'stream' | 'odysseus' | 'embed'

export type DeviceTier = 'phone' | 'tablet' | 'desktop' | 'large' | 'tv' | 'kiosk'

export interface PanelState {
  id: string
  type: PanelType
  component: string   // route path key e.g. 'dashboard', 'containers', or embed URL
  title: string
  icon: string
  layoutMode: LayoutMode
  x: number; y: number; w: number; h: number
  savedX: number; savedY: number; savedW: number; savedH: number
  zIndex: number
  pinned: boolean
  workspaceIndex: number
  groupId?: string
  tabIndex?: number
}

export type SnapZone =
  | 'left-half' | 'right-half' | 'top-half' | 'bottom-half'
  | 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right'
  | 'fullscreen'

const PANEL_CAPS: Record<DeviceTier, number> = {
  phone: 1,
  tablet: 3,
  desktop: 5,
  large: 8,
  tv: 5,
  kiosk: 5,
}

interface AiosStore {
  panels: PanelState[]
  focusedId: string | null
  activeWorkspace: 0 | 1 | 2 | 3
  splitPair: [string, string] | null
  splitRatio: number
  deviceTier: DeviceTier
  _zCounter: number

  openPanel: (panel: Omit<PanelState, 'id' | 'zIndex'>) => void
  closePanel: (id: string) => void
  focusPanel: (id: string) => void
  updatePanel: (id: string, updates: Partial<PanelState>) => void
  movePanel: (id: string, x: number, y: number) => void
  resizePanel: (id: string, x: number, y: number, w: number, h: number) => void
  snapPanel: (id: string, mode: LayoutMode) => void
  minimizePanel: (id: string) => void
  maximizePanel: (id: string) => void
  restorePanel: (id: string) => void
  togglePinned: (id: string) => void
  setWorkspace: (index: 0 | 1 | 2 | 3) => void
  sendToWorkspace: (id: string, i: number) => void
  setSplitPair: (pair: [string, string] | null) => void
  coupleAsSplit: (leftId: string, rightId: string) => void
  uncoupleSplit: () => void
  setSplitRatio: (ratio: number) => void
  setDeviceTier: (tier: DeviceTier) => void
  closeAll: () => void
}

export const newPanelId = () => `panel-${crypto.randomUUID()}`

let zCounter = 100

function snapGeometry(zone: SnapZone, vw: number, vh: number, statusH = 28, dockH = 56) {
  const canvas = vh - statusH - dockH
  const halfW = vw / 2
  const halfH = canvas / 2
  const top = statusH
  switch (zone) {
    case 'left-half':    return { x: 0,     y: top,           w: halfW,  h: canvas }
    case 'right-half':   return { x: halfW, y: top,           w: halfW,  h: canvas }
    case 'top-half':     return { x: 0,     y: top,           w: vw,     h: halfH }
    case 'bottom-half':  return { x: 0,     y: top + halfH,   w: vw,     h: halfH }
    case 'top-left':     return { x: 0,     y: top,           w: halfW,  h: halfH }
    case 'top-right':    return { x: halfW, y: top,           w: halfW,  h: halfH }
    case 'bottom-left':  return { x: 0,     y: top + halfH,   w: halfW,  h: halfH }
    case 'bottom-right': return { x: halfW, y: top + halfH,   w: halfW,  h: halfH }
    case 'fullscreen':   return { x: 0,     y: top,           w: vw,     h: canvas }
  }
}

export const useAiosStore = create<AiosStore>()(
  persist(
    (set, get) => ({
      panels: [],
      focusedId: null,
      activeWorkspace: 0,
      splitPair: null,
      splitRatio: 0.5,
      deviceTier: 'desktop' as DeviceTier,
      _zCounter: 100,

      openPanel: (panelData) => {
        const z = ++zCounter
        const { panels, deviceTier } = get()
        const cap = PANEL_CAPS[deviceTier]
        const newId = newPanelId()
        const newPanel: PanelState = { ...panelData, id: newId, zIndex: z }

        let updatedPanels = [...panels]

        // If at or above cap, minimize the oldest non-pinned visible panel
        const visible = updatedPanels.filter(
          (p) => p.layoutMode !== 'minimized' && !p.pinned
        )
        if (visible.length >= cap) {
          const oldest = visible.reduce(
            (min, p) => (p.zIndex < min.zIndex ? p : min),
            visible[0]
          )
          updatedPanels = updatedPanels.map((p) =>
            p.id === oldest.id ? { ...p, layoutMode: 'minimized' as LayoutMode } : p
          )
        }

        set({ panels: [...updatedPanels, newPanel], focusedId: newId })
      },

      closePanel: (id) => set((s) => {
        const panels = s.panels.filter((p) => p.id !== id)
        const splitPair = s.splitPair?.includes(id) ? null : s.splitPair
        const focusedId = s.focusedId === id
          ? (panels[panels.length - 1]?.id ?? null)
          : s.focusedId
        return { panels, splitPair, focusedId }
      }),

      focusPanel: (id) => {
        const z = ++zCounter
        set((s) => ({
          focusedId: id,
          panels: s.panels.map((p) => p.id === id ? { ...p, zIndex: z } : p),
        }))
      },

      updatePanel: (id, updates) => set((s) => ({
        panels: s.panels.map((p) => (p.id === id ? { ...p, ...updates } : p)),
      })),

      movePanel: (id, x, y) => set((s) => ({
        panels: s.panels.map((p) => p.id === id ? { ...p, x, y, layoutMode: 'floating' } : p),
      })),

      resizePanel: (id, x, y, w, h) => set((s) => ({
        panels: s.panels.map((p) => p.id === id ? { ...p, x, y, w, h, layoutMode: 'floating' } : p),
      })),

      snapPanel: (id, mode) => {
        // For snap zones that have geometry, compute it
        const snapZones: SnapZone[] = [
          'left-half', 'right-half', 'top-half', 'bottom-half',
          'top-left', 'top-right', 'bottom-left', 'bottom-right', 'fullscreen',
        ]
        const isSnapZone = snapZones.includes(mode as SnapZone)
        const geo = isSnapZone
          ? snapGeometry(mode as SnapZone, window.innerWidth, window.innerHeight)
          : undefined

        set((s) => ({
          panels: s.panels.map((p) => {
            if (p.id !== id) return p
            const isCurrentlyFloating = p.layoutMode === 'floating'
            return {
              ...p,
              savedX: isCurrentlyFloating ? p.x : p.savedX,
              savedY: isCurrentlyFloating ? p.y : p.savedY,
              savedW: isCurrentlyFloating ? p.w : p.savedW,
              savedH: isCurrentlyFloating ? p.h : p.savedH,
              layoutMode: mode,
              ...(geo ?? {}),
            }
          }),
        }))

        // Auto-couple split when two panels are snapped left/right
        if (mode === 'left-half' || mode === 'right-half') {
          const { panels, splitPair } = get()
          const counterMode = mode === 'left-half' ? 'right-half' : 'left-half'
          const other = panels.find((p) => p.id !== id && !splitPair && p.layoutMode === counterMode)
          if (other) {
            const leftId = mode === 'left-half' ? id : other.id
            const rightId = mode === 'left-half' ? other.id : id
            get().coupleAsSplit(leftId, rightId)
          }
        }
      },

      minimizePanel: (id) => set((s) => ({
        panels: s.panels.map((p) => p.id === id ? { ...p, layoutMode: 'minimized' } : p),
        focusedId: s.focusedId === id ? null : s.focusedId,
      })),

      maximizePanel: (id) => set((s) => ({
        panels: s.panels.map((p) => {
          if (p.id !== id) return p
          const isCurrentlyFloating = p.layoutMode === 'floating'
          return {
            ...p,
            savedX: isCurrentlyFloating ? p.x : p.savedX,
            savedY: isCurrentlyFloating ? p.y : p.savedY,
            savedW: isCurrentlyFloating ? p.w : p.savedW,
            savedH: isCurrentlyFloating ? p.h : p.savedH,
            layoutMode: 'fullscreen' as LayoutMode,
          }
        }),
      })),

      restorePanel: (id) => {
        const panel = get().panels.find((p) => p.id === id)
        if (!panel) return
        get().focusPanel(id)
        set((s) => ({
          panels: s.panels.map((p) => p.id === id
            ? { ...p, layoutMode: 'floating', x: p.savedX || p.x, y: p.savedY || p.y, w: p.savedW || p.w, h: p.savedH || p.h }
            : p),
        }))
      },

      togglePinned: (id) => set((s) => ({
        panels: s.panels.map((p) => p.id === id ? { ...p, pinned: !p.pinned } : p),
      })),

      setWorkspace: (i) => set({ activeWorkspace: i }),

      sendToWorkspace: (id, i) => set((s) => ({
        panels: s.panels.map((p) => p.id === id ? { ...p, workspaceIndex: i } : p),
      })),

      setSplitPair: (pair) => set({ splitPair: pair }),

      coupleAsSplit: (leftId, rightId) => set({ splitPair: [leftId, rightId] }),

      uncoupleSplit: () => set({ splitPair: null }),

      setSplitRatio: (r) => set({ splitRatio: Math.max(0.15, Math.min(0.85, r)) }),

      setDeviceTier: (tier) => set({ deviceTier: tier }),

      closeAll: () => set({ panels: [], focusedId: null, splitPair: null }),
    }),
    {
      name: 'aios-panels',
      version: 2,
      storage: createJSONStorage(() => sessionStorage),
      partialize: (s) => ({
        panels: s.panels,
        activeWorkspace: s.activeWorkspace,
        splitPair: s.splitPair,
        splitRatio: s.splitRatio,
        deviceTier: s.deviceTier,
      }),
    },
  ),
)
