import { create } from 'zustand'
import { persist, createJSONStorage } from 'zustand/middleware'

export type LayoutMode =
  | 'floating' | 'left-half' | 'right-half' | 'top-half' | 'bottom-half'
  | 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right'
  | 'fullscreen' | 'minimized' | 'sheet' | 'tile'

export type PanelType = 'app' | 'stream' | 'odysseus' | 'embed'

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

interface AiosStore {
  panels: PanelState[]
  focusedId: string | null
  activeWorkspace: 0 | 1 | 2 | 3
  splitPair: [string, string] | null
  splitRatio: number
  _zCounter: number

  openPanel: (panel: Omit<PanelState, 'zIndex'>) => void
  closePanel: (id: string) => void
  focusPanel: (id: string) => void
  movePanel: (id: string, x: number, y: number) => void
  resizePanel: (id: string, x: number, y: number, w: number, h: number) => void
  snapPanel: (id: string, zone: SnapZone) => void
  minimizePanel: (id: string) => void
  restorePanel: (id: string) => void
  togglePinned: (id: string) => void
  setWorkspace: (i: 0 | 1 | 2 | 3) => void
  sendToWorkspace: (id: string, i: number) => void
  coupleAsSplit: (leftId: string, rightId: string) => void
  uncoupleSplit: () => void
  setSplitRatio: (r: number) => void
  closeAll: () => void
}

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
      _zCounter: 100,

      openPanel: (panel) => {
        const z = ++zCounter
        set((s) => ({ panels: [...s.panels, { ...panel, zIndex: z }], focusedId: panel.id }))
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

      movePanel: (id, x, y) => set((s) => ({
        panels: s.panels.map((p) => p.id === id ? { ...p, x, y, layoutMode: 'floating' } : p),
      })),

      resizePanel: (id, x, y, w, h) => set((s) => ({
        panels: s.panels.map((p) => p.id === id ? { ...p, x, y, w, h, layoutMode: 'floating' } : p),
      })),

      snapPanel: (id, zone) => {
        const vw = window.innerWidth
        const vh = window.innerHeight
        const geo = snapGeometry(zone, vw, vh)
        set((s) => ({
          panels: s.panels.map((p) => {
            if (p.id !== id) return p
            return { ...p, layoutMode: zone, ...geo, savedX: p.x, savedY: p.y, savedW: p.w, savedH: p.h }
          }),
        }))
        // Auto-couple split
        const { panels, splitPair } = get()
        const other = panels.find((p) =>
          p.id !== id &&
          !splitPair &&
          ((zone === 'left-half' && p.layoutMode === 'right-half') ||
           (zone === 'right-half' && p.layoutMode === 'left-half'))
        )
        if (other) get().coupleAsSplit(zone === 'left-half' ? id : other.id, zone === 'left-half' ? other.id : id)
      },

      minimizePanel: (id) => set((s) => ({
        panels: s.panels.map((p) => p.id === id ? { ...p, layoutMode: 'minimized' } : p),
        focusedId: s.focusedId === id ? null : s.focusedId,
      })),

      restorePanel: (id) => {
        const panel = get().panels.find((p) => p.id === id)
        if (!panel) return
        const mode = panel.savedW > 0 ? 'floating' : 'floating'
        get().focusPanel(id)
        set((s) => ({
          panels: s.panels.map((p) => p.id === id
            ? { ...p, layoutMode: mode, x: p.savedX || p.x, y: p.savedY || p.y, w: p.savedW || p.w, h: p.savedH || p.h }
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

      coupleAsSplit: (leftId, rightId) => set({ splitPair: [leftId, rightId] }),

      uncoupleSplit: () => set({ splitPair: null }),

      setSplitRatio: (r) => set({ splitRatio: Math.max(0.15, Math.min(0.85, r)) }),

      closeAll: () => set({ panels: [], focusedId: null, splitPair: null }),
    }),
    {
      name: 'vt-aios',
      storage: createJSONStorage(() => sessionStorage),
      partialize: (s) => ({
        panels: s.panels,
        activeWorkspace: s.activeWorkspace,
        splitPair: s.splitPair,
        splitRatio: s.splitRatio,
      }),
    },
  ),
)

let _panelCounter = 0
export const newPanelId = () => `panel-${++_panelCounter}`
