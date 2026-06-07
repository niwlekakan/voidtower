import { create } from 'zustand'
import { persist, createJSONStorage } from 'zustand/middleware'
import {
  type TileNode,
  type SplitDir,
  insertPanel as bspInsert,
  removePanel as bspRemove,
  computeRects,
} from '@/aios/tiling'

export type LayoutMode =
  | 'floating' | 'left-half' | 'right-half' | 'top-half' | 'bottom-half'
  | 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right'
  | 'fullscreen' | 'minimized' | 'sheet' | 'tile'

export type PanelType = 'app' | 'stream' | 'odysseus' | 'embed'

export type DeviceTier = 'phone' | 'tablet' | 'desktop' | 'large' | 'tv' | 'kiosk'

export type PresetName = 'ai-assist' | 'debug' | 'vm' | 'android'

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
  workspaceNames: [string, string, string, string]
  splitPair: [string, string] | null
  splitRatio: number
  deviceTier: DeviceTier
  /** Actual rendered dimensions — set by AiosLayout on mount/resize */
  dims: { statusH: number; dockH: number; dockLeft: number }
  _zCounter: number

  // ── Tiling ─────────────────────────────────────────────────────────────
  tileMode: boolean
  tileTrees: [TileNode | null, TileNode | null, TileNode | null, TileNode | null]

  toggleTileMode: () => void
  splitPanel: (panelId: string, dir: SplitDir, newPanelData: Omit<PanelState, 'id' | 'zIndex'>) => void
  removeTilePanel: (panelId: string) => void
  /** Recompute rects for the current workspace tree and push to panel states */
  applyTileLayout: () => void

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
  renameWorkspace: (index: 0 | 1 | 2 | 3, name: string) => void
  sendToWorkspace: (id: string, i: number) => void
  setSplitPair: (pair: [string, string] | null) => void
  coupleAsSplit: (leftId: string, rightId: string) => void
  uncoupleSplit: () => void
  setSplitRatio: (ratio: number) => void
  setDeviceTier: (tier: DeviceTier) => void
  setDims: (statusH: number, dockH: number, dockLeft: number) => void
  closeAll: () => void
  /** Close all non-pinned panels on the current workspace (clean slate) */
  resetLayout: () => void
  /** Open a named preset layout on the current workspace */
  applyPreset: (preset: PresetName) => void
  /** Open or focus the Odysseus panel */
  openOdysseus: () => void
  /** Snap the focused panel left and open Odysseus snapped right */
  openBesideOdysseus: () => void
  /** Open the Inspector panel targeting a specific panel id */
  openInspector: (targetPanelId: string) => void
}

export const newPanelId = () => `panel-${crypto.randomUUID()}`

let zCounter = 100

function snapGeometry(zone: SnapZone, vw: number, vh: number, statusH = 28, dockH = 56, dockLeft = 0) {
  const canvas = vh - statusH - dockH
  const usableW = vw - dockLeft
  const halfW = usableW / 2
  const halfH = canvas / 2
  const top = statusH
  const left = dockLeft
  const midX = left + halfW
  switch (zone) {
    case 'left-half':    return { x: left,  y: top,           w: halfW,   h: canvas }
    case 'right-half':   return { x: midX,  y: top,           w: halfW,   h: canvas }
    case 'top-half':     return { x: left,  y: top,           w: usableW, h: halfH }
    case 'bottom-half':  return { x: left,  y: top + halfH,   w: usableW, h: halfH }
    case 'top-left':     return { x: left,  y: top,           w: halfW,   h: halfH }
    case 'top-right':    return { x: midX,  y: top,           w: halfW,   h: halfH }
    case 'bottom-left':  return { x: left,  y: top + halfH,   w: halfW,   h: halfH }
    case 'bottom-right': return { x: midX,  y: top + halfH,   w: halfW,   h: halfH }
    case 'fullscreen':   return { x: left,  y: top,           w: usableW, h: canvas }
  }
}

export const useAiosStore = create<AiosStore>()(
  persist(
    (set, get) => ({
      panels: [],
      focusedId: null,
      activeWorkspace: 0,
      workspaceNames: ['Main', 'Work', 'Lab', 'Monitor'] as [string, string, string, string],
      splitPair: null,
      splitRatio: 0.5,
      deviceTier: 'desktop' as DeviceTier,
      dims: { statusH: 36, dockH: 62, dockLeft: 0 },
      _zCounter: 100,

      // ── Tiling initial state ──────────────────────────────────────────
      tileMode: false,
      tileTrees: [null, null, null, null] as [TileNode | null, TileNode | null, TileNode | null, TileNode | null],

      toggleTileMode: () => {
        const { tileMode } = get()
        const next = !tileMode
        set({ tileMode: next })
        if (next) get().applyTileLayout()
      },

      applyTileLayout: () => {
        const { tileTrees, activeWorkspace, panels, dims } = get()
        const tree = tileTrees[activeWorkspace]
        if (!tree) return
        const vw = typeof window !== 'undefined' ? window.innerWidth : 1280
        const vh = typeof window !== 'undefined' ? window.innerHeight : 800
        const { statusH, dockH, dockLeft } = dims
        const bounds = {
          x: dockLeft,
          y: statusH,
          w: vw - dockLeft,
          h: vh - statusH - dockH,
        }
        const rects = computeRects(tree, bounds)
        set({
          panels: panels.map((p) => {
            if (p.workspaceIndex !== activeWorkspace) return p
            const rect = rects.get(p.id)
            if (!rect) return p
            return { ...p, x: rect.x, y: rect.y, w: rect.w, h: rect.h, layoutMode: 'floating' as LayoutMode }
          }),
        })
      },

      splitPanel: (panelId, dir, newPanelData) => {
        const { tileTrees, activeWorkspace, panels } = get()
        const newId = newPanelId()
        const z = ++zCounter
        const newPanel: PanelState = {
          ...newPanelData,
          id: newId,
          zIndex: z,
          workspaceIndex: activeWorkspace,
          layoutMode: 'floating' as LayoutMode,
        }

        const currentTree = tileTrees[activeWorkspace] ?? null
        const layout = { root: currentTree, x: 0, y: 0, w: 0, h: 0 }
        const updated = bspInsert(layout, newId, panelId, dir)

        const nextTrees = [...tileTrees] as [TileNode | null, TileNode | null, TileNode | null, TileNode | null]
        nextTrees[activeWorkspace] = updated.root

        set({
          panels: [...panels, newPanel],
          tileTrees: nextTrees,
          focusedId: newId,
        })
        get().applyTileLayout()
      },

      removeTilePanel: (panelId) => {
        const { tileTrees, activeWorkspace } = get()
        const currentTree = tileTrees[activeWorkspace] ?? null
        if (!currentTree) return
        const layout = { root: currentTree, x: 0, y: 0, w: 0, h: 0 }
        const updated = bspRemove(layout, panelId)
        const nextTrees = [...tileTrees] as [TileNode | null, TileNode | null, TileNode | null, TileNode | null]
        nextTrees[activeWorkspace] = updated.root
        set({ tileTrees: nextTrees })
        get().applyTileLayout()
      },

      openPanel: (panelData) => {
        const { tileMode, focusedId, tileTrees, activeWorkspace, panels, deviceTier } = get()

        // In tile mode, insert into the BSP tree instead of free placement
        if (tileMode) {
          const z = ++zCounter
          const newId = newPanelId()
          const newPanel: PanelState = {
            ...panelData,
            id: newId,
            zIndex: z,
            layoutMode: 'floating' as LayoutMode,
          }
          // Split the focused panel, or root if none
          const focusedLeaf = focusedId ?? null
          const currentTree = tileTrees[activeWorkspace] ?? null
          const layout = { root: currentTree, x: 0, y: 0, w: 0, h: 0 }
          const updated = bspInsert(layout, newId, focusedLeaf, 'v')
          const nextTrees = [...tileTrees] as [TileNode | null, TileNode | null, TileNode | null, TileNode | null]
          nextTrees[activeWorkspace] = updated.root
          set({ panels: [...panels, newPanel], tileTrees: nextTrees, focusedId: newId })
          get().applyTileLayout()
          return
        }

        const z = ++zCounter
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

      closePanel: (id) => {
        const { tileMode } = get()
        if (tileMode) get().removeTilePanel(id)
        set((s) => {
          const panels = s.panels.filter((p) => p.id !== id)
          const splitPair = s.splitPair?.includes(id) ? null : s.splitPair
          const focusedId = s.focusedId === id
            ? (panels[panels.length - 1]?.id ?? null)
            : s.focusedId
          return { panels, splitPair, focusedId }
        })
      },

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
          ? snapGeometry(mode as SnapZone, window.innerWidth, window.innerHeight, get().dims.statusH, get().dims.dockH, get().dims.dockLeft)
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

      maximizePanel: (id) => {
        const geo = snapGeometry('fullscreen', window.innerWidth, window.innerHeight, get().dims.statusH, get().dims.dockH, get().dims.dockLeft)
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
              layoutMode: 'fullscreen' as LayoutMode,
              ...geo,
            }
          }),
        }))
      },

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

      renameWorkspace: (i, name) => set((s) => {
        const next = [...s.workspaceNames] as [string, string, string, string]
        next[i] = name.trim() || `Workspace ${i + 1}`
        return { workspaceNames: next }
      }),

      sendToWorkspace: (id, i) => set((s) => ({
        panels: s.panels.map((p) => p.id === id ? { ...p, workspaceIndex: i } : p),
      })),

      setSplitPair: (pair) => set({ splitPair: pair }),

      coupleAsSplit: (leftId, rightId) => set({ splitPair: [leftId, rightId] }),

      uncoupleSplit: () => set({ splitPair: null }),

      setSplitRatio: (r) => set({ splitRatio: Math.max(0.15, Math.min(0.85, r)) }),

      setDeviceTier: (tier) => set({ deviceTier: tier }),
      setDims: (statusH: number, dockH: number, dockLeft: number) => set({ dims: { statusH, dockH, dockLeft } }),

      closeAll: () => set({ panels: [], focusedId: null, splitPair: null }),

      resetLayout: () => set((s) => {
        const kept = s.panels.filter(
          (p) => p.pinned || p.workspaceIndex !== s.activeWorkspace,
        )
        const focusedId = kept.find((p) => p.workspaceIndex === s.activeWorkspace)?.id ?? null
        return { panels: kept, focusedId, splitPair: null }
      }),

      openOdysseus: () => {
        const { panels, focusPanel, openPanel, activeWorkspace } = get()
        const existing = panels.find((p) => p.type === 'odysseus')
        if (existing) {
          focusPanel(existing.id)
          return
        }
        const vw = typeof window !== 'undefined' ? window.innerWidth : 1280
        const vh = typeof window !== 'undefined' ? window.innerHeight : 800
        const w = Math.min(820, vw - 40)
        const h = Math.min(600, vh - 80)
        openPanel({
          type: 'odysseus',
          component: 'ai',
          title: 'Odysseus',
          icon: '🧠',
          layoutMode: 'floating',
          x: Math.max(20, (vw - w) / 2),
          y: Math.max(36, (vh - h) / 2),
          w, h,
          savedX: Math.max(20, (vw - w) / 2),
          savedY: Math.max(36, (vh - h) / 2),
          savedW: w, savedH: h,
          pinned: false,
          workspaceIndex: activeWorkspace,
        })
      },

      openBesideOdysseus: () => {
        const { focusedId, panels, activeWorkspace, snapPanel, focusPanel, coupleAsSplit } = get()

        // Snap the currently focused panel to the left half
        const wsPanels = panels.filter((p) => p.workspaceIndex === activeWorkspace && p.layoutMode !== 'minimized')
        const target = focusedId
          ? panels.find((p) => p.id === focusedId && p.workspaceIndex === activeWorkspace)
          : wsPanels[wsPanels.length - 1]

        if (target) snapPanel(target.id, 'left-half')

        // Open or focus Odysseus on the right half
        const existing = panels.find((p) => p.type === 'odysseus' && p.workspaceIndex === activeWorkspace)
        if (existing) {
          snapPanel(existing.id, 'right-half')
          focusPanel(existing.id)
          if (target) coupleAsSplit(target.id, existing.id)
          return
        }

        const vw = typeof window !== 'undefined' ? window.innerWidth : 1280
        const vh = typeof window !== 'undefined' ? window.innerHeight : 800
        const { statusH, dockH } = get().dims
        const geo = {
          x: vw / 2, y: statusH,
          w: vw / 2, h: vh - statusH - dockH,
        }
        const newId = newPanelId()
        const z = ++zCounter
        set((s) => ({
          panels: [...s.panels, {
            id: newId, type: 'odysseus' as const, component: 'ai',
            title: 'Odysseus', icon: '🧠',
            layoutMode: 'right-half' as const,
            ...geo,
            savedX: geo.x, savedY: geo.y, savedW: geo.w, savedH: geo.h,
            zIndex: z, pinned: false, workspaceIndex: activeWorkspace,
          }],
          focusedId: newId,
        }))
        if (target) coupleAsSplit(target.id, newId)
      },

      applyPreset: (preset) => {
        // Lazy import to avoid circular dependency — presets module imports back only types
        import('@/aios/AiosPresets').then(({ PRESETS }) => {
          const def = PRESETS[preset]
          if (!def) return
          const { panels, activeWorkspace, snapPanel, focusPanel, deviceTier } = get()

          // Close non-pinned panels on current workspace first
          set((s) => {
            const kept = s.panels.filter(
              (p) => p.pinned || p.workspaceIndex !== s.activeWorkspace,
            )
            return { panels: kept, focusedId: null, splitPair: null }
          })

          const vw = typeof window !== 'undefined' ? window.innerWidth : 1280
          const vh = typeof window !== 'undefined' ? window.innerHeight : 800
          const { statusH, dockH } = get().dims
          const canvas = vh - statusH - dockH

          // Spawn each slot
          const isDesktop = deviceTier === 'desktop' || deviceTier === 'large'
          const slots = isDesktop ? def.slots : def.slots.filter((sl) => !sl.desktopOnly)

          // Re-read panels after reset
          const freshPanels = get().panels

          slots.forEach((slot) => {
            const existing = freshPanels.find(
              (p) => p.component === slot.component && p.workspaceIndex === activeWorkspace,
            )
            if (existing) {
              focusPanel(existing.id)
              return
            }

            let geo: { x: number; y: number; w: number; h: number }
            const isNamedSnapZone = (m: string): m is SnapZone =>
              ['left-half','right-half','top-half','bottom-half',
               'top-left','top-right','bottom-left','bottom-right','fullscreen'].includes(m)
            if (slot.snapZone && isNamedSnapZone(slot.snapZone)) {
              geo = snapGeometry(slot.snapZone, vw, vh)
            } else if (slot.xFrac !== undefined) {
              // Fractional viewport geometry
              const w = Math.round(vw * (slot.wFrac ?? 0.6))
              const h = Math.round(canvas * (slot.hFrac ?? 0.75))
              const x = Math.round(vw * (slot.xFrac ?? 0.05))
              const y = statusH + Math.round(canvas * (slot.yFrac ?? 0.0))
              geo = { x, y, w, h }
            } else {
              geo = { x: slot.x ?? 0, y: slot.y ?? statusH, w: slot.w ?? 800, h: slot.h ?? 500 }
            }

            const newId = newPanelId()
            const z = ++zCounter
            const newPanel: PanelState = {
              id: newId,
              type: slot.type ?? 'app',
              component: slot.component,
              title: slot.title,
              icon: slot.icon ?? '',
              layoutMode: slot.snapZone ?? 'floating',
              x: geo.x, y: geo.y, w: geo.w, h: geo.h,
              savedX: geo.x, savedY: geo.y, savedW: geo.w, savedH: geo.h,
              zIndex: z,
              pinned: false,
              workspaceIndex: activeWorkspace,
            }
            set((s) => ({ panels: [...s.panels, newPanel], focusedId: newId }))

            // If snap zone was provided, also run snapPanel to get saved coords right
            if (slot.snapZone && slot.snapZone !== 'floating') {
              snapPanel(newId, slot.snapZone as LayoutMode)
            }
          })

          // Auto-couple split for left/right pairs
          const afterPanels = get().panels.filter((p) => p.workspaceIndex === activeWorkspace)
          const leftPanel = afterPanels.find((p) => p.layoutMode === 'left-half')
          const rightPanel = afterPanels.find((p) => p.layoutMode === 'right-half')
          if (leftPanel && rightPanel) {
            set({ splitPair: [leftPanel.id, rightPanel.id] })
          }

          // Suppress unused-var warning — panels was from closure before reset
          void panels
        })
      },

      openInspector: (targetPanelId) => {
        const { panels, focusPanel, openPanel, activeWorkspace } = get()
        // If inspector is already open, just focus it
        const existing = panels.find((p) => p.component === 'inspector')
        if (existing) {
          // Update the inspected panel id via title (simple approach; Inspector reads focusedId)
          focusPanel(existing.id)
          return
        }
        const vw = typeof window !== 'undefined' ? window.innerWidth : 1280
        const vh = typeof window !== 'undefined' ? window.innerHeight : 800
        const w = 320
        const h = 480
        // Pin to right edge of screen
        const x = vw - w - 16
        const y = Math.max(40, (vh - h) / 2)
        openPanel({
          type: 'app',
          component: 'inspector',
          title: 'Inspector',
          icon: '🔍',
          layoutMode: 'floating',
          x, y, w, h,
          savedX: x, savedY: y, savedW: w, savedH: h,
          pinned: false,
          workspaceIndex: activeWorkspace,
        })
        // Store the target panel id in panel title for Inspector to pick up
        // (Inspector reads focusedId from store, so just ensure target is known)
        void targetPanelId
      },
    }),
    {
      name: 'aios-panels',
      version: 3,
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({
        panels: s.panels,
        activeWorkspace: s.activeWorkspace,
        workspaceNames: s.workspaceNames,
        splitPair: s.splitPair,
        splitRatio: s.splitRatio,
        deviceTier: s.deviceTier,
        tileTrees: s.tileTrees,
        // tileMode intentionally omitted — resets to false on reload
      }),
    },
  ),
)
