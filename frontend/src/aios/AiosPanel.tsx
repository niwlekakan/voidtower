import React, { useRef, useState, useCallback } from 'react'
import { createPortal } from 'react-dom'
import { useAiosStore, type PanelState, type LayoutMode, type SnapZone } from '@/aios/store/aios'
import { getSnapZone, snapPreviewRect } from '@/aios/hooks/useSnapZones'
import type { DeviceTier } from '@/aios/hooks/useDeviceTier'

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

export interface AiosPanelProps {
  panel: PanelState
  tier: DeviceTier
  children: React.ReactNode
  statusBarH?: number
  dockH?: number
}

// ---------------------------------------------------------------------------
// Snap geometry map  (non-floating, non-minimized)
// ---------------------------------------------------------------------------

const SNAP_STYLES: Partial<Record<LayoutMode, React.CSSProperties>> = {
  'left-half':    { left: 0,      top: 0,      width: '50vw',  height: '100vh' },
  'right-half':   { left: '50vw', top: 0,      width: '50vw',  height: '100vh' },
  'top-half':     { left: 0,      top: 0,      width: '100vw', height: '50vh'  },
  'bottom-half':  { left: 0,      top: '50vh', width: '100vw', height: '50vh'  },
  'top-left':     { left: 0,      top: 0,      width: '50vw',  height: '50vh'  },
  'top-right':    { left: '50vw', top: 0,      width: '50vw',  height: '50vh'  },
  'bottom-left':  { left: 0,      top: '50vh', width: '50vw',  height: '50vh'  },
  'bottom-right': { left: '50vw', top: '50vh', width: '50vw',  height: '50vh'  },
  'fullscreen':   { left: 0,      top: 0,      width: '100vw', height: '100vh' },
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
}

const SNAP_PRESETS: { label: string; zone: SnapZone }[] = [
  { label: 'Left half',   zone: 'left-half'   },
  { label: 'Right half',  zone: 'right-half'  },
  { label: 'Top half',    zone: 'top-half'    },
  { label: 'Bottom half', zone: 'bottom-half' },
  { label: 'Fullscreen',  zone: 'fullscreen'  },
]

function PanelContextMenu({ panelId, x, y, onClose }: ContextMenuProps) {
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
  const { closePanel, minimizePanel } = useAiosStore()
  const dragStartY = useRef<number | null>(null)
  const dragDeltaY = useRef(0)
  const [offsetY, setOffsetY] = useState(0)

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

  return (
    <div
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
        backdropFilter: 'blur(24px)',
        WebkitBackdropFilter: 'blur(24px)',
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

// ---------------------------------------------------------------------------
// Main AiosPanel
// ---------------------------------------------------------------------------

export default function AiosPanel({ panel, tier, children }: AiosPanelProps) {
  const {
    focusPanel,
    closePanel,
    minimizePanel,
    snapPanel,
    movePanel,
    resizePanel,
    togglePinned,
    restorePanel,
    focusedId,
  } = useAiosStore()

  const isFocused = focusedId === panel.id

  // Drag refs
  const panelRef = useRef<HTMLDivElement>(null)
  const titlebarRef = useRef<HTMLDivElement>(null)
  const draggingRef = useRef(false)
  const dragStart = useRef({ px: 0, py: 0, ox: 0, oy: 0 })
  const [snapPreview, setSnapPreview] = useState<SnapZone | null>(null)

  // Resize refs
  const resizingRef = useRef(false)
  const resizeEdgeRef = useRef<ResizeEdge | null>(null)
  const resizeStart = useRef({ px: 0, py: 0, x: 0, y: 0, w: 0, h: 0 })

  // Context menu
  const [ctxMenu, setCtxMenu] = useState<{ x: number; y: number } | null>(null)

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
  const isFloating  = layoutMode === 'floating'
  const isMinimized = layoutMode === 'minimized'
  const cornerSize  = tier === 'tablet' ? 24 : 8

  // Panel CSS
  let panelStyle: React.CSSProperties = {
    position: 'fixed',
    zIndex: panel.pinned ? 9999 : zIndex,
    display: 'flex',
    flexDirection: 'column',
    borderRadius: 10,
    overflow: 'hidden',
    background: 'var(--bg-panel, rgba(0,0,0,0.6))',
    backdropFilter: 'blur(24px)',
    WebkitBackdropFilter: 'blur(24px)',
    border: `1px solid ${isFocused ? 'var(--accent-primary,#6366f1)' : 'rgba(255,255,255,0.08)'}`,
    boxShadow: isFocused
      ? '0 0 0 1px var(--accent-primary,#6366f1), 0 12px 48px rgba(0,0,0,0.55)'
      : '0 8px 32px rgba(0,0,0,0.4)',
    outline: 'none',
    transition: isFloating ? 'box-shadow 0.15s, border-color 0.15s' : 'all 0.18s cubic-bezier(.4,0,.2,1)',
  }

  if (isFloating || isMinimized) {
    panelStyle = {
      ...panelStyle,
      transform: `translate(${x}px, ${y}px)`,
      width: w,
      height: isMinimized ? 40 : h,
    }
  } else {
    const snapStyle = SNAP_STYLES[layoutMode]
    if (snapStyle) panelStyle = { ...panelStyle, ...snapStyle }
  }

  // ── Titlebar drag ─────────────────────────────────────────────────────────

  const onTitlePointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return
    if ((e.target as HTMLElement).closest('button')) return
    if (!isFloating) return
    e.preventDefault()
    focusPanel(panel.id)
    draggingRef.current = true
    dragStart.current = { px: e.clientX, py: e.clientY, ox: panel.x, oy: panel.y }
    ;(e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId)
    if (panelRef.current) panelRef.current.style.willChange = 'transform'
  }, [isFloating, panel.id, panel.x, panel.y, focusPanel])

  const onTitlePointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return
    e.preventDefault()
    const dx = e.clientX - dragStart.current.px
    const dy = e.clientY - dragStart.current.py
    movePanel(panel.id, dragStart.current.ox + dx, dragStart.current.oy + dy)
    if (tier === 'desktop' || tier === 'large' || tier === 'tablet') {
      setSnapPreview(getSnapZone(e.clientX, e.clientY))
    }
  }, [panel.id, movePanel, tier])

  const onTitlePointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return
    draggingRef.current = false
    ;(e.currentTarget as HTMLDivElement).releasePointerCapture(e.pointerId)
    if (panelRef.current) panelRef.current.style.willChange = 'auto'
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
    if (layoutMode === 'fullscreen') restorePanel(panel.id)
    else snapPanel(panel.id, 'fullscreen')
  }, [layoutMode, panel.id, restorePanel, snapPanel])

  // ── Context menu ──────────────────────────────────────────────────────────

  const onTitleContextMenu = useCallback((e: React.MouseEvent) => {
    e.preventDefault()
    setCtxMenu({ x: e.clientX, y: e.clientY })
  }, [])

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

      {/* Context menu */}
      {ctxMenu && (
        <PanelContextMenu
          panelId={panel.id}
          x={ctxMenu.x}
          y={ctxMenu.y}
          onClose={() => setCtxMenu(null)}
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
      >
        {/* Titlebar */}
        <div
          ref={titlebarRef}
          style={{
            display: 'flex', alignItems: 'center', gap: 8,
            height: 40, padding: '0 10px', flexShrink: 0,
            background: 'var(--bg-elevated, rgba(0,0,0,0.4))',
            borderBottom: '1px solid rgba(255,255,255,0.08)',
            borderRadius: '10px 10px 0 0',
            cursor: isFloating ? 'grab' : 'default',
            userSelect: 'none',
            touchAction: 'none',
          }}
          onPointerDown={onTitlePointerDown}
          onPointerMove={onTitlePointerMove}
          onPointerUp={onTitlePointerUp}
          onDoubleClick={onTitleDblClick}
          onContextMenu={onTitleContextMenu}
        >
          <span style={{ fontSize: 14, pointerEvents: 'none' }}>{panel.icon}</span>
          <span style={{
            flex: 1, fontSize: 12, fontWeight: 600,
            color: 'rgba(255,255,255,0.9)',
            overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap',
            pointerEvents: 'none',
          }}>
            {panel.title}
          </span>

          {/* Pin */}
          <TitleBtn
            label="⊤"
            title={panel.pinned ? 'Unpin' : 'Pin on top'}
            active={panel.pinned}
            onClick={() => togglePinned(panel.id)}
          />

          {/* Minimize */}
          <TitleBtn label="─" title="Minimize" onClick={() => minimizePanel(panel.id)} />

          {/* Maximize / Restore */}
          <TitleBtn
            label="⬜"
            title={layoutMode === 'fullscreen' ? 'Restore' : 'Maximize'}
            onClick={() => layoutMode === 'fullscreen' ? restorePanel(panel.id) : snapPanel(panel.id, 'fullscreen')}
          />

          {/* Close */}
          <TitleBtn label="✕" title="Close" danger onClick={() => closePanel(panel.id)} />
        </div>

        {/* Body – hidden when minimized */}
        {!isMinimized && (
          <div
            style={{ flex: 1, overflow: 'hidden', position: 'relative' }}
            onPointerMove={resizingRef.current ? onResizePointerMove : undefined}
            onPointerUp={resizingRef.current ? onResizePointerUp : undefined}
          >
            {children}
          </div>
        )}

        {/* 8-zone resize handles — floating panels, non-minimized */}
        {isFloating && !isMinimized && RESIZE_ZONES.map((zone) => (
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
// Small reusable titlebar button
// ---------------------------------------------------------------------------

interface TitleBtnProps {
  label: string
  title: string
  onClick: () => void
  active?: boolean
  danger?: boolean
}

function TitleBtn({ label, title, onClick, active, danger }: TitleBtnProps) {
  return (
    <button
      title={title}
      onClick={(e) => { e.stopPropagation(); onClick() }}
      onPointerDown={(e) => e.stopPropagation()}
      style={{
        ...titleBtnBase,
        color: active
          ? 'var(--accent-primary, #6366f1)'
          : danger
          ? 'var(--text-muted, rgba(255,255,255,0.4))'
          : 'var(--text-muted, rgba(255,255,255,0.4))',
      }}
      onMouseEnter={(e) => {
        const el = e.currentTarget
        if (danger) el.style.color = 'var(--accent-danger, #f87171)'
        else if (!active) el.style.color = 'rgba(255,255,255,0.85)'
      }}
      onMouseLeave={(e) => {
        const el = e.currentTarget
        if (active) el.style.color = 'var(--accent-primary, #6366f1)'
        else if (danger) el.style.color = 'var(--text-muted, rgba(255,255,255,0.4))'
        else el.style.color = 'var(--text-muted, rgba(255,255,255,0.4))'
      }}
    >
      {label}
    </button>
  )
}

const titlebatStaticStyle: React.CSSProperties = {
  display: 'flex', alignItems: 'center', gap: 8,
  height: 40, padding: '0 10px', flexShrink: 0,
  background: 'var(--bg-elevated, rgba(0,0,0,0.4))',
  borderBottom: '1px solid rgba(255,255,255,0.08)',
  borderRadius: '10px 10px 0 0',
}
