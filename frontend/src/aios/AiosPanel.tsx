import React, { useRef, useState, useCallback } from 'react'
import { createPortal } from 'react-dom'
import { useAiosStore, type PanelState, type LayoutMode, type SnapZone } from '@/aios/store/aios'
import { getSnapZone, snapPreviewRect } from '@/aios/hooks/useSnapZones'
import type { DeviceTier } from '@/aios/hooks/useDeviceTier'
import { useTouchGestures } from '@/aios/hooks/useTouchGestures'
import AiBadge from '@/components/ui/AiBadge'
import type { AiLevel } from '@/components/ui/AiBadge'

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

export interface AiosPanelProps {
  panel: PanelState
  tier: DeviceTier
  children: React.ReactNode
  statusBarH?: number
  dockH?: number
  /** AI integration level shown in panel chrome */
  aiLevel?: AiLevel
}

// ---------------------------------------------------------------------------
// AI level derived from known component keys
// ---------------------------------------------------------------------------

const COMPONENT_AI_LEVELS: Record<string, AiLevel> = {
  ai:         'native',
  services:   'aware',
  containers: 'aware',
  terminal:   'aware',
}

function deriveAiLevel(component: string, explicit?: AiLevel): AiLevel | undefined {
  if (explicit !== undefined) return explicit
  return COMPONENT_AI_LEVELS[component]
}

// ---------------------------------------------------------------------------
// Snap geometry map  (non-floating, non-minimized)
// ---------------------------------------------------------------------------

// All snap positions respect the status bar (top) and dock (bottom).
// --aios-status-h / --aios-dock-h are set on the layout root; fallbacks match known defaults.
const SH  = 'var(--aios-status-h,36px)'
const DH  = 'var(--aios-dock-h,62px)'
const DL  = 'var(--aios-dock-left,0px)'
// Canvas = full viewport minus status bar, dock, and left dock column
const CW       = `calc(100vw - ${DL})`
const CH       = `calc(100vh - ${SH} - ${DH})`
const HALF_W   = `calc((100vw - ${DL}) / 2)`
const HALF_H   = `calc((100vh - ${SH} - ${DH}) / 2)`
const MID_X    = `calc(${DL} + (100vw - ${DL}) / 2)`
const MID_Y    = `calc(${SH} + (100vh - ${SH} - ${DH}) / 2)`

const SNAP_STYLES: Partial<Record<LayoutMode, React.CSSProperties>> = {
  'left-half':    { left: DL,    top: SH,    width: HALF_W, height: CH     },
  'right-half':   { left: MID_X, top: SH,    width: HALF_W, height: CH     },
  'top-half':     { left: DL,    top: SH,    width: CW,     height: HALF_H },
  'bottom-half':  { left: DL,    top: MID_Y, width: CW,     height: HALF_H },
  'top-left':     { left: DL,    top: SH,    width: HALF_W, height: HALF_H },
  'top-right':    { left: MID_X, top: SH,    width: HALF_W, height: HALF_H },
  'bottom-left':  { left: DL,    top: MID_Y, width: HALF_W, height: HALF_H },
  'bottom-right': { left: MID_X, top: MID_Y, width: HALF_W, height: HALF_H },
  // fullscreen omitted — geometry stored on panel state via snapGeometry()
}

// ---------------------------------------------------------------------------
// Resize zones
// ---------------------------------------------------------------------------

type ResizeEdge = 'nw' | 'ne' | 'sw' | 'se' | 'n' | 's' | 'w' | 'e'

interface ResizeZoneDesc {
  id: ResizeEdge
  cursor: string
  // base style – width/height for corners will be overridden by cornerSize
  style: React.CSSProperties
}

const RESIZE_ZONES: ResizeZoneDesc[] = [
  { id: 'nw', cursor: 'nw-resize', style: { top: 0,    left: 0                               } },
  { id: 'ne', cursor: 'ne-resize', style: { top: 0,    right: 0                              } },
  { id: 'sw', cursor: 'sw-resize', style: { bottom: 0, left: 0                               } },
  { id: 'se', cursor: 'se-resize', style: { bottom: 0, right: 0                              } },
  { id: 'n',  cursor: 'n-resize',  style: { top: 0,    left: 8, right: 8,   height: 8        } },
  { id: 's',  cursor: 's-resize',  style: { bottom: 0, left: 8, right: 8,   height: 8        } },
  { id: 'w',  cursor: 'w-resize',  style: { top: 8,    left: 0, bottom: 8,  width: 8         } },
  { id: 'e',  cursor: 'e-resize',  style: { top: 8,    right: 0, bottom: 8, width: 8         } },
]

const MIN_W = 320
const MIN_H = 240

// ---------------------------------------------------------------------------
// Snap preview overlay (portalled)
// ---------------------------------------------------------------------------

function SnapPreviewOverlay({ zone }: { zone: SnapZone }) {
  const rect = snapPreviewRect(zone)
  return (
    <div
      style={{
        position: 'fixed',
        left: rect.x,
        top: rect.y,
        width: rect.w,
        height: rect.h,
        background: 'var(--accent-primary, #6366f1)',
        opacity: 0.18,
        border: '2px solid var(--accent-primary, #6366f1)',
        borderRadius: 10,
        pointerEvents: 'none',
        zIndex: 99998,
        transition: 'all 0.1s ease',
      }}
    />
  )
}

// ---------------------------------------------------------------------------
// Titlebar context menu
// ---------------------------------------------------------------------------

interface ContextMenuProps {
  panelId: string
  x: number
  y: number
  onClose: () => void
  onInspect: () => void
}

const SNAP_PRESETS: { label: string; zone: SnapZone }[] = [
  { label: 'Left half',   zone: 'left-half'   },
  { label: 'Right half',  zone: 'right-half'  },
  { label: 'Top half',    zone: 'top-half'    },
  { label: 'Bottom half', zone: 'bottom-half' },
  { label: 'Fullscreen',  zone: 'fullscreen'  },
]

function PanelContextMenu({ panelId, x, y, onClose, onInspect }: ContextMenuProps) {
  const { snapPanel, closePanel } = useAiosStore()

  const run = (fn: () => void) => { fn(); onClose() }

  return createPortal(
    <>
      {/* backdrop */}
      <div
        style={{ position: 'fixed', inset: 0, zIndex: 99999 }}
        onPointerDown={onClose}
      />
      <div
        style={{
          position: 'fixed',
          left: x,
          top: y,
          zIndex: 100000,
          background: 'rgba(0,0,0,0.92)',
          border: '1px solid rgba(255,255,255,0.12)',
          borderRadius: 8,
          boxShadow: '0 8px 32px rgba(0,0,0,0.5)',
          padding: '4px 0',
          minWidth: 160,
          fontSize: 12,
        }}
        onPointerDown={(e) => e.stopPropagation()}
      >
        {SNAP_PRESETS.map(({ label, zone }) => (
          <button
            key={zone}
            onClick={() => run(() => snapPanel(panelId, zone))}
            style={menuItemStyle}
            onMouseEnter={(e) => (e.currentTarget.style.background = 'rgba(255,255,255,0.08)')}
            onMouseLeave={(e) => (e.currentTarget.style.background = 'transparent')}
          >
            {label}
          </button>
        ))}
        <div style={{ height: 1, background: 'rgba(255,255,255,0.1)', margin: '4px 0' }} />
        <button
          onClick={() => run(() =>
            useAiosStore.setState((s) => ({
              panels: s.panels.map((p) => p.id === panelId ? { ...p, zIndex: 9999 } : p),
            }))
          )}
          style={menuItemStyle}
          onMouseEnter={(e) => (e.currentTarget.style.background = 'rgba(255,255,255,0.08)')}
          onMouseLeave={(e) => (e.currentTarget.style.background = 'transparent')}
        >
          Always on top
        </button>
        <div style={{ height: 1, background: 'rgba(255,255,255,0.1)', margin: '4px 0' }} />
        <button
          onClick={() => run(onInspect)}
          style={menuItemStyle}
          onMouseEnter={(e) => (e.currentTarget.style.background = 'rgba(255,255,255,0.08)')}
          onMouseLeave={(e) => (e.currentTarget.style.background = 'transparent')}
        >
          Inspect
        </button>
        <div style={{ height: 1, background: 'rgba(255,255,255,0.1)', margin: '4px 0' }} />
        <button
          onClick={() => run(() => closePanel(panelId))}
          style={{ ...menuItemStyle, color: 'var(--accent-danger, #f87171)' }}
          onMouseEnter={(e) => (e.currentTarget.style.background = 'rgba(255,255,255,0.08)')}
          onMouseLeave={(e) => (e.currentTarget.style.background = 'transparent')}
        >
          Close
        </button>
      </div>
    </>,
    document.body,
  )
}

const menuItemStyle: React.CSSProperties = {
  display: 'block',
  width: '100%',
  textAlign: 'left',
  padding: '6px 12px',
  background: 'transparent',
  border: 'none',
  color: 'rgba(255,255,255,0.82)',
  cursor: 'pointer',
  fontSize: 12,
}

// ---------------------------------------------------------------------------
// Phone-tier bottom sheet
// ---------------------------------------------------------------------------

function PhoneSheetPanel({ panel, children }: { panel: PanelState; children: React.ReactNode }) {
  const { closePanel, minimizePanel, restorePanel } = useAiosStore()
  const dragStartY = useRef<number | null>(null)
  const dragDeltaY = useRef(0)
  const [offsetY, setOffsetY] = useState(0)
  const sheetRef = useRef<HTMLDivElement>(null)

  // Wire touch gestures: swipe down → minimize, long-press title → context menu
  useTouchGestures(sheetRef as React.RefObject<HTMLElement>, {
    onSwipeDown: () => {
      if (panel.layoutMode !== 'minimized') minimizePanel(panel.id)
    },
  })

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    dragStartY.current = e.clientY
    dragDeltaY.current = 0
    ;(e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId)
  }

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (dragStartY.current === null) return
    const dy = Math.max(0, e.clientY - dragStartY.current)
    dragDeltaY.current = dy
    setOffsetY(dy)
  }

  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    ;(e.currentTarget as HTMLDivElement).releasePointerCapture(e.pointerId)
    if (dragDeltaY.current > 80) minimizePanel(panel.id)
    dragStartY.current = null
    dragDeltaY.current = 0
    setOffsetY(0)
  }

  // If minimized, render a collapsed pill that can be tapped to restore
  if (panel.layoutMode === 'minimized') {
    return (
      <div
        style={{
          position: 'fixed',
          bottom: 72,
          left: '50%',
          transform: 'translateX(-50%)',
          zIndex: panel.zIndex,
          background: 'rgba(0,0,0,0.85)',
          backdropFilter: 'var(--vt-blur)',
          WebkitBackdropFilter: 'var(--vt-blur)',
          border: '1px solid rgba(255,255,255,0.12)',
          borderRadius: 20,
          padding: '6px 16px',
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          cursor: 'pointer',
        }}
        onClick={() => restorePanel(panel.id)}
      >
        <span style={{ fontSize: 13 }}>{panel.icon}</span>
        <span style={{ fontSize: 12, color: 'rgba(255,255,255,0.8)' }}>{panel.title}</span>
      </div>
    )
  }

  return (
    <div
      ref={sheetRef}
      style={{
        position: 'fixed',
        bottom: 0,
        left: 0,
        right: 0,
        height: '90vh',
        transform: `translateY(${offsetY}px)`,
        willChange: offsetY > 0 ? 'transform' : 'auto',
        zIndex: panel.zIndex,
        display: 'flex',
        flexDirection: 'column',
        background: 'var(--bg-panel, rgba(0,0,0,0.8))',
        backdropFilter: 'var(--vt-blur)',
        WebkitBackdropFilter: 'var(--vt-blur)',
        borderTop: '1px solid rgba(255,255,255,0.1)',
        borderRadius: '16px 16px 0 0',
        boxShadow: '0 -8px 40px rgba(0,0,0,0.5)',
        transition: offsetY === 0 ? 'transform 200ms cubic-bezier(.4,0,.2,1)' : undefined,
      }}
    >
      {/* Drag handle */}
      <div
        style={{ height: 20, touchAction: 'none', cursor: 'row-resize', flexShrink: 0, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
      >
        <div style={{ width: 40, height: 4, borderRadius: 2, background: 'rgba(255,255,255,0.3)', margin: '8px auto 0' }} />
      </div>

      {/* Titlebar */}
      <div style={{
        display: 'flex', alignItems: 'center', gap: 8,
        height: 40, padding: '0 12px', flexShrink: 0,
        borderBottom: '1px solid rgba(255,255,255,0.08)',
      }}>
        <span style={{ fontSize: 15 }}>{panel.icon}</span>
        <span style={{ flex: 1, fontSize: 13, fontWeight: 600, color: 'rgba(255,255,255,0.9)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
          {panel.title}
        </span>
        <button
          onClick={() => closePanel(panel.id)}
          style={{ ...titleBtnBase, color: 'rgba(255,255,255,0.4)' }}
          onMouseEnter={(e) => (e.currentTarget.style.color = 'var(--accent-danger,#f87171)')}
          onMouseLeave={(e) => (e.currentTarget.style.color = 'rgba(255,255,255,0.4)')}
        >
          ✕
        </button>
      </div>

      <div style={{ flex: 1, overflow: 'hidden' }}>{children}</div>
    </div>
  )
}

const titleBtnBase: React.CSSProperties = {
  width: 24, height: 24,
  display: 'flex', alignItems: 'center', justifyContent: 'center',
  background: 'none', border: 'none', cursor: 'pointer', borderRadius: 4, flexShrink: 0,
  fontSize: 11, transition: 'color 0.12s',
}

// macOS-style traffic light dot colors
const DOT_CLOSE    = '#ff5f57'
const DOT_MINIMIZE = '#febc2e'
const DOT_MAXIMIZE = '#28c840'

// ---------------------------------------------------------------------------
// Main AiosPanel
// ---------------------------------------------------------------------------

export default function AiosPanel({ panel, tier, children, aiLevel: aiLevelProp }: AiosPanelProps) {
  const {
    focusPanel,
    closePanel,
    minimizePanel,
    snapPanel,
    movePanel,
    resizePanel,
    togglePinned,
    restorePanel,
    openInspector,
    focusedId,
    tileMode,
    panels,
    activeWorkspace,
  } = useAiosStore()

  const isFocused = focusedId === panel.id
  const resolvedAiLevel = deriveAiLevel(panel.component, aiLevelProp)

  // Drag refs
  const panelRef = useRef<HTMLDivElement>(null)
  const titlebarRef = useRef<HTMLDivElement>(null)
  const draggingRef = useRef(false)
  const dragStart = useRef({ px: 0, py: 0, ox: 0, oy: 0 })
  const [snapPreview, setSnapPreview] = useState<SnapZone | null>(null)

  // Snap guide lines for freeform mode
  type GuideAxis = 'h' | 'v'
  interface SnapGuide { axis: GuideAxis; pos: number }
  const [snapGuides, setSnapGuides] = useState<SnapGuide[]>([])

  // Resize refs
  const resizingRef = useRef(false)
  const resizeEdgeRef = useRef<ResizeEdge | null>(null)
  const resizeStart = useRef({ px: 0, py: 0, x: 0, y: 0, w: 0, h: 0 })

  // Context menu
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null)

  // Wire touch gestures to title bar: swipe down → minimize
  useTouchGestures(titlebarRef as React.RefObject<HTMLElement>, {
    onSwipeDown: () => {
      if (panel.layoutMode !== 'minimized') minimizePanel(panel.id)
    },
  })

  // NOTE: all hooks below must stay above the phone/tv early returns —
  // React requires the same hooks to run on every render regardless of tier.

  // ── Titlebar drag ─────────────────────────────────────────────────────────

  const onTitlePointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return
    if ((e.target as HTMLElement).closest('button')) return
    if (tileMode) return  // drag disabled in tile mode
    // Dragging a fullscreen panel restores it to floating first
    if (panel.layoutMode === 'fullscreen') { restorePanel(panel.id); return }
    if (panel.layoutMode !== 'floating') return
    e.preventDefault()
    focusPanel(panel.id)
    draggingRef.current = true
    dragStart.current = { px: e.clientX, py: e.clientY, ox: panel.x, oy: panel.y }
    ;(e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId)
    if (panelRef.current) {
      panelRef.current.style.willChange = 'transform'
      panelRef.current.style.backdropFilter = 'none'
      ;(panelRef.current.style as any).webkitBackdropFilter = 'none'
    }
  }, [tileMode, panel.id, panel.x, panel.y, panel.layoutMode, focusPanel, restorePanel])

  const onTitlePointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return
    e.preventDefault()
    const dx = e.clientX - dragStart.current.px
    const dy = e.clientY - dragStart.current.py
    const nx = dragStart.current.ox + dx
    const ny = dragStart.current.oy + dy
    movePanel(panel.id, nx, ny)
    if (tier === 'desktop' || tier === 'large' || tier === 'tablet') {
      setSnapPreview(getSnapZone(e.clientX, e.clientY))
    }
    // Snap guide lines — check edges of other panels on the same workspace
    const SNAP_THRESHOLD = 80
    const otherPanels = panels.filter(
      (p) => p.id !== panel.id && p.workspaceIndex === activeWorkspace && p.layoutMode !== 'minimized',
    )
    const guides: SnapGuide[] = []
    const myEdges = { left: nx, right: nx + panel.w, top: ny, bottom: ny + panel.h }
    for (const other of otherPanels) {
      const edges = [
        { axis: 'v' as const, pos: other.x },
        { axis: 'v' as const, pos: other.x + other.w },
        { axis: 'h' as const, pos: other.y },
        { axis: 'h' as const, pos: other.y + other.h },
      ]
      for (const edge of edges) {
        if (edge.axis === 'v') {
          if (Math.abs(myEdges.left - edge.pos) < SNAP_THRESHOLD ||
              Math.abs(myEdges.right - edge.pos) < SNAP_THRESHOLD) {
            if (!guides.some((g) => g.axis === 'v' && g.pos === edge.pos)) {
              guides.push(edge)
            }
          }
        } else {
          if (Math.abs(myEdges.top - edge.pos) < SNAP_THRESHOLD ||
              Math.abs(myEdges.bottom - edge.pos) < SNAP_THRESHOLD) {
            if (!guides.some((g) => g.axis === 'h' && g.pos === edge.pos)) {
              guides.push(edge)
            }
          }
        }
      }
    }
    setSnapGuides(guides)
  }, [panel.id, panel.w, panel.h, panels, activeWorkspace, movePanel, tier])

  const onTitlePointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return
    draggingRef.current = false
    ;(e.currentTarget as HTMLDivElement).releasePointerCapture(e.pointerId)
    if (panelRef.current) {
      panelRef.current.style.willChange = 'auto'
      panelRef.current.style.backdropFilter = 'var(--vt-blur)'
      ;(panelRef.current.style as any).webkitBackdropFilter = 'var(--vt-blur)'
    }
    setSnapGuides([])
    if (snapPreview) {
      snapPanel(panel.id, snapPreview)
      setSnapPreview(null)
    }
  }, [panel.id, snapPanel, snapPreview])

  // ── Resize ────────────────────────────────────────────────────────────────

  const onResizePointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>, edge: ResizeEdge) => {
    e.preventDefault()
    e.stopPropagation()
    focusPanel(panel.id)
    resizingRef.current = true
    resizeEdgeRef.current = edge
    resizeStart.current = { px: e.clientX, py: e.clientY, x: panel.x, y: panel.y, w: panel.w, h: panel.h }
    ;(e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId)
    if (panelRef.current) panelRef.current.style.willChange = 'transform'
  }, [panel, focusPanel])

  const onResizePointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!resizingRef.current || !resizeEdgeRef.current) return
    e.preventDefault()
    const { px, py, x: sx, y: sy, w: sw, h: sh } = resizeStart.current
    const dx = e.clientX - px
    const dy = e.clientY - py
    const vw = window.innerWidth
    const vh = window.innerHeight
    const edge = resizeEdgeRef.current

    let nx = sx, ny = sy, nw = sw, nh = sh

    if (edge.includes('e')) nw = Math.max(MIN_W, Math.min(vw - sx, sw + dx))
    if (edge.includes('s')) nh = Math.max(MIN_H, Math.min(vh - sy, sh + dy))
    if (edge.includes('w')) { nw = Math.max(MIN_W, sw - dx); nx = sx + (sw - nw) }
    if (edge.includes('n')) { nh = Math.max(MIN_H, sh - dy); ny = sy + (sh - nh) }

    resizePanel(panel.id, nx, ny, nw, nh)
  }, [panel.id, resizePanel])

  const onResizePointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!resizingRef.current) return
    resizingRef.current = false
    resizeEdgeRef.current = null
    ;(e.currentTarget as HTMLDivElement).releasePointerCapture(e.pointerId)
    if (panelRef.current) panelRef.current.style.willChange = 'auto'
  }, [])

  // ── Keyboard ──────────────────────────────────────────────────────────────

  const onKeyDown = useCallback((e: React.KeyboardEvent) => {
    if (!isFocused) return
    if (e.key === 'Escape') minimizePanel(panel.id)
  }, [isFocused, panel.id, minimizePanel])

  // ── Double-click titlebar ─────────────────────────────────────────────────

  const onTitleDblClick = useCallback(() => {
    if (panel.layoutMode === 'fullscreen') restorePanel(panel.id)
    else snapPanel(panel.id, 'fullscreen')
  }, [panel.layoutMode, panel.id, restorePanel, snapPanel])

  // ── Context menu ──────────────────────────────────────────────────────────

  const onTitleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    setCtxMenu({ x: e.clientX, y: e.clientY })
  }, [])

  // ── Inspect ───────────────────────────────────────────────────────────────

  const handleInspect = useCallback(() => {
    openInspector(panel.id)
  }, [panel.id, openInspector])

  // ── Phone tier ─────────────────────────────────────────────────────────────
  if (tier === 'phone') {
    return <PhoneSheetPanel panel={panel}>{children}</PhoneSheetPanel>
  }

  // ── TV / tile tier ─────────────────────────────────────────────────────────
  if (tier === 'tv') {
    return (
      <div
        onPointerDown={() => focusPanel(panel.id)}
        tabIndex={0}
        role="region"
        aria-label={panel.title}
        data-focused={isFocused ? '' : undefined}
        style={{
          position: 'static',
          display: 'flex', flexDirection: 'column',
          borderRadius: 10, overflow: 'hidden',
          background: 'var(--bg-panel, rgba(0,0,0,0.6))',
          backdropFilter: 'blur(20px)',
          border: `1px solid ${isFocused ? 'var(--accent-primary,#6366f1)' : 'rgba(255,255,255,0.08)'}`,
          boxShadow: isFocused ? '0 0 0 2px var(--accent-primary,#6366f1)' : '0 8px 32px rgba(0,0,0,0.4)',
          outline: 'none',
        }}
      >
        <div style={titlebatStaticStyle}>
          <span style={{ fontSize: 14 }}>{panel.icon}</span>
          <span style={{ flex: 1, fontSize: 12, fontWeight: 600, color: 'rgba(255,255,255,0.9)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {panel.title}
          </span>
          {resolvedAiLevel && resolvedAiLevel !== 'none' && (
            <AiBadge level={resolvedAiLevel} compact />
          )}
          <button
            onClick={() => closePanel(panel.id)}
            style={{ ...titleBtnBase, color: 'rgba(255,255,255,0.4)' }}
            onMouseEnter={(e) => (e.currentTarget.style.color = 'var(--accent-danger,#f87171)')}
            onMouseLeave={(e) => (e.currentTarget.style.color = 'rgba(255,255,255,0.4)')}
          >
            ✕
          </button>
        </div>
        <div style={{ flex: 1, overflow: 'hidden' }}>{children}</div>
      </div>
    )
  }

  // ── Desktop / tablet / large / kiosk ───────────────────────────────────────

  const { layoutMode, x, y, w, h, zIndex } = panel
  const isFloating   = layoutMode === 'floating'
  const isFullscreen = layoutMode === 'fullscreen'
  const isMinimized  = layoutMode === 'minimized'
  const cornerSize  = tier === 'tablet' ? 24 : 8

  // Accent line color: red for 'danger'-type panels, default accent-primary
  const accentLineColor = 'var(--accent-primary, #8b5cf6)'

  // Panel CSS
  let panelStyle: React.CSSProperties = {
    position: 'fixed',
    zIndex: panel.pinned ? 9999 : zIndex,
    display: 'flex',
    flexDirection: 'column',
    borderRadius: 10,
    overflow: 'hidden',
    background: 'linear-gradient(135deg, rgba(139,92,246,0.05) 0%, transparent 100%), var(--bg-panel, #0b0d14)',
    backdropFilter: 'var(--vt-blur)',
    WebkitBackdropFilter: 'var(--vt-blur)',
    border: isFocused
      ? '1px solid rgba(139,92,246,0.4)'
      : '1px solid rgba(255,255,255,0.06)',
    boxShadow: isFocused
      ? '0 0 0 1px rgba(139,92,246,0.5), 0 8px 40px rgba(0,0,0,0.5)'
      : '0 4px 24px rgba(0,0,0,0.3)',
    outline: 'none',
    transition: isFloating ? 'box-shadow 0.15s, border-color 0.15s' : 'all 0.18s cubic-bezier(.4,0,.2,1)',
  }

  if (isFloating || isMinimized || isFullscreen) {
    // fullscreen geometry is stored on panel state by snapGeometry() in the store,
    // so it correctly respects the status-bar and dock insets.
    panelStyle = {
      ...panelStyle,
      transform: `translate(${x}px, ${y}px)`,
      width: w,
      height: isMinimized ? 40 : h,
      // Remove border-radius in fullscreen for a clean edge-to-edge look
      borderRadius: isFullscreen ? 0 : 10,
    }
  } else {
    const snapStyle = SNAP_STYLES[layoutMode]
    if (snapStyle) panelStyle = { ...panelStyle, ...snapStyle }
  }

  // ── Resize handle size helper ─────────────────────────────────────────────

  const resizeZoneStyle = (zone: ResizeZoneDesc): React.CSSProperties => {
    const base: React.CSSProperties = {
      position: 'absolute',
      cursor: zone.cursor,
      touchAction: 'none',
      zIndex: 10,
      ...zone.style,
    }
    const isCorner = zone.id.length === 2
    if (isCorner) return { ...base, width: cornerSize, height: cornerSize }
    return base
  }

  return (
    <>
      {/* Snap preview portal */}
      {snapPreview && createPortal(
        <SnapPreviewOverlay zone={snapPreview} />,
        document.body,
      )}

      {/* Snap guide lines — freeform drag only */}
      {snapGuides.length > 0 && createPortal(
        <>
          {snapGuides.map((guide, i) => (
            <div
              key={i}
              style={{
                position: 'fixed',
                pointerEvents: 'none',
                zIndex: 99997,
                background: 'var(--accent-secondary, #06b6d4)',
                ...(guide.axis === 'v'
                  ? { left: guide.pos, top: 0, width: 1, height: '100vh' }
                  : { top: guide.pos, left: 0, height: 1, width: '100vw' }),
              }}
            />
          ))}
        </>,
        document.body,
      )}

      {/* Context menu */}
      {ctxMenu && (
        <PanelContextMenu
          panelId={panel.id}
          x={ctxMenu.x}
          y={ctxMenu.y}
          onClose={() => setCtxMenu(null)}
          onInspect={handleInspect}
        />
      )}

      <div
        ref={panelRef}
        style={panelStyle}
        onPointerDown={() => focusPanel(panel.id)}
        onKeyDown={onKeyDown}
        tabIndex={0}
        role="dialog"
        aria-label={panel.title}
        data-focused={isFocused ? '' : undefined}
      >
        {/* 2px accent line at top edge — focused only */}
        {isFocused && (
          <div style={{
            position: 'absolute',
            top: 0, left: 0, right: 0,
            height: 2,
            background: accentLineColor,
            pointerEvents: 'none',
            zIndex: 5,
            borderRadius: '10px 10px 0 0',
          }} />
        )}

        {/* Titlebar */}
        <TitleBar
          titlebarRef={titlebarRef}
          panel={panel}
          isFocused={isFocused}
          isFloating={isFloating}
          layoutMode={layoutMode}
          resolvedAiLevel={resolvedAiLevel}
          onPointerDown={onTitlePointerDown}
          onPointerMove={onTitlePointerMove}
          onPointerUp={onTitlePointerUp}
          onDoubleClick={onTitleDblClick}
          onContextMenu={onTitleContextMenu}
          onClose={() => closePanel(panel.id)}
          onMinimize={() => minimizePanel(panel.id)}
          onMaximize={() => layoutMode === 'fullscreen' ? restorePanel(panel.id) : snapPanel(panel.id, 'fullscreen')}
          onTogglePin={() => togglePinned(panel.id)}
        />

        {/* Body – hidden when minimized */}
        {!isMinimized && (
          <div
            style={{
              flex: 1, overflow: 'hidden', position: 'relative',
              // Add inner breathing room except in fullscreen (edge-to-edge)
              padding: isFullscreen ? 0 : 12,
            }}
            onPointerMove={resizingRef.current ? onResizePointerMove : undefined}
            onPointerUp={resizingRef.current ? onResizePointerUp : undefined}
          >
            {children}
          </div>
        )}

        {/* 8-zone resize handles — floating panels, non-minimized, freeform only */}
        {isFloating && !isMinimized && !tileMode && RESIZE_ZONES.map((zone) => (
          <div
            key={zone.id}
            style={resizeZoneStyle(zone)}
            onPointerDown={(e) => onResizePointerDown(e, zone.id)}
            onPointerMove={onResizePointerMove}
            onPointerUp={onResizePointerUp}
          />
        ))}
      </div>
    </>
  )
}

// ---------------------------------------------------------------------------
// macOS-style traffic-light titlebar
// ---------------------------------------------------------------------------

interface TitleBarProps {
  titlebarRef: React.RefObject<HTMLDivElement>
  panel: PanelState
  isFocused: boolean
  isFloating: boolean
  layoutMode: LayoutMode
  resolvedAiLevel: AiLevel | undefined
  onPointerDown: React.PointerEventHandler<HTMLDivElement>
  onPointerMove: React.PointerEventHandler<HTMLDivElement>
  onPointerUp: React.PointerEventHandler<HTMLDivElement>
  onDoubleClick: () => void
  onContextMenu: React.MouseEventHandler<HTMLDivElement>
  onClose: () => void
  onMinimize: () => void
  onMaximize: () => void
  onTogglePin: () => void
}

function TitleBar({
  titlebarRef, panel, isFocused, isFloating, layoutMode,
  resolvedAiLevel,
  onPointerDown, onPointerMove, onPointerUp, onDoubleClick, onContextMenu,
  onClose, onMinimize, onMaximize, onTogglePin,
}: TitleBarProps) {
  const [hovering, setHovering] = useState(false)

  return (
    <div
      ref={titlebarRef}
      style={{
        display: 'flex', alignItems: 'center', gap: 6,
        height: 30, padding: '0 8px', flexShrink: 0,
        background: 'rgba(10,8,20,0.95)',
        backdropFilter: 'var(--vt-blur)',
        WebkitBackdropFilter: 'var(--vt-blur)',
        borderBottom: '1px solid rgba(255,255,255,0.06)',
        borderRadius: '10px 10px 0 0',
        cursor: isFloating ? 'grab' : 'default',
        userSelect: 'none',
        touchAction: 'none',
      }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onDoubleClick={onDoubleClick}
      onContextMenu={onContextMenu}
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
    >
      {/* Traffic light dots */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexShrink: 0 }}>
        <TrafficDot
          color={DOT_CLOSE}
          label="×"
          title="Close"
          showLabel={hovering}
          onClick={(e) => { e.stopPropagation(); onClose() }}
        />
        <TrafficDot
          color={DOT_MINIMIZE}
          label="−"
          title="Minimize"
          showLabel={hovering}
          onClick={(e) => { e.stopPropagation(); onMinimize() }}
        />
        <TrafficDot
          color={DOT_MAXIMIZE}
          label="⤢"
          title={layoutMode === 'fullscreen' ? 'Restore' : 'Maximize'}
          showLabel={hovering}
          onClick={(e) => { e.stopPropagation(); onMaximize() }}
        />
      </div>

      {/* Icon + title */}
      <span style={{ fontSize: 12, pointerEvents: 'none', opacity: 0.7 }}>{panel.icon}</span>
      <span style={{
        flex: 1, fontSize: 11, fontWeight: 500,
        color: isFocused ? 'rgba(255,255,255,0.85)' : 'rgba(255,255,255,0.45)',
        overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
        pointerEvents: 'none',
        letterSpacing: '0.01em',
      }}>
        {panel.title}
      </span>

      {/* AI badge */}
      {resolvedAiLevel && resolvedAiLevel !== 'none' && (
        <div style={{ pointerEvents: 'none', flexShrink: 0 }}>
          <AiBadge level={resolvedAiLevel} compact />
        </div>
      )}

      {/* Pin indicator (subtle, icon-only) */}
      {panel.pinned && (
        <button
          title="Unpin"
          onClick={(e) => { e.stopPropagation(); onTogglePin() }}
          onPointerDown={(e) => e.stopPropagation()}
          style={{
            background: 'none', border: 'none', cursor: 'pointer',
            color: 'var(--accent-primary, #8b5cf6)',
            fontSize: 10, padding: '0 2px', flexShrink: 0,
          }}
        >
          ⊤
        </button>
      )}
    </div>
  )
}

// ---------------------------------------------------------------------------
// Traffic light dot
// ---------------------------------------------------------------------------

interface TrafficDotProps {
  color: string
  label: string
  title: string
  showLabel: boolean
  onClick: React.MouseEventHandler<HTMLButtonElement>
}

function TrafficDot({ color, label, title, showLabel, onClick }: TrafficDotProps) {
  return (
    <button
      title={title}
      onClick={onClick}
      onPointerDown={(e) => e.stopPropagation()}
      style={{
        width: 10, height: 10,
        borderRadius: '50%',
        background: color,
        border: 'none', cursor: 'pointer',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        flexShrink: 0, padding: 0,
        fontSize: 7, fontWeight: 700,
        color: 'rgba(0,0,0,0.7)',
        lineHeight: 1,
        transition: 'opacity 0.12s',
      }}
    >
      {showLabel ? label : null}
    </button>
  )
}

const titlebatStaticStyle: React.CSSProperties = {
  display: 'flex', alignItems: 'center', gap: 6,
  height: 30, padding: '0 8px', flexShrink: 0,
  background: 'rgba(10,8,20,0.95)',
  backdropFilter: 'blur(40px)',
  WebkitBackdropFilter: 'blur(40px)',
  borderBottom: '1px solid rgba(255,255,255,0.06)',
  borderRadius: '10px 10px 0 0',
}
