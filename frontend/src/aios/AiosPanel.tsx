import { useRef, useState, useCallback } from 'react'
import { createPortal } from 'react-dom'
import { X, Minus, Maximize2, Pin, PanelLeft } from 'lucide-react'
import { useAiosStore, type PanelState, type SnapZone } from '@/aios/store/aios'
import { getSnapZone, snapPreviewRect } from '@/aios/hooks/useSnapZones'
import type { DeviceTier } from '@/aios/hooks/useDeviceTier'

type ResizeZone = 'n'|'s'|'e'|'w'|'nw'|'ne'|'sw'|'se'

const CURSOR: Record<ResizeZone, string> = {
  n:'ns-resize', s:'ns-resize', e:'ew-resize', w:'ew-resize',
  nw:'nw-resize', ne:'ne-resize', sw:'sw-resize', se:'se-resize',
}

const RESIZE_HIT = 8 // px

interface Props {
  panel: PanelState
  tier: DeviceTier
  children: React.ReactNode
  statusBarH: number
  dockH: number
}

export default function AiosPanel({ panel, tier, children, statusBarH, dockH }: Props) {
  const { focusPanel, movePanel, resizePanel, snapPanel, minimizePanel, closePanel, togglePinned, focusedId } = useAiosStore()

  const dragRef = useRef<{ startX: number; startY: number; panelX: number; panelY: number } | null>(null)
  const resizeRef = useRef<{
    zone: ResizeZone; startX: number; startY: number
    startW: number; startH: number; startPX: number; startPY: number
  } | null>(null)

  const [snapPreview, setSnapPreview] = useState<SnapZone | null>(null)
  const isFocused = focusedId === panel.id
  const isFloating = panel.layoutMode === 'floating'
  const isMinimized = panel.layoutMode === 'minimized'

  // Geometry from layout mode
  const style = useLayoutStyle(panel, statusBarH, dockH)

  // ── Titlebar drag ────────────────────────────────────────────────────────────
  const onTitlePointerDown = useCallback((e: React.PointerEvent) => {
    if (e.button !== 0 || !isFloating) return
    e.preventDefault()
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    dragRef.current = { startX: e.clientX, startY: e.clientY, panelX: panel.x, panelY: panel.y }
    focusPanel(panel.id)
  }, [isFloating, panel.x, panel.y, panel.id, focusPanel])

  const onTitlePointerMove = useCallback((e: React.PointerEvent) => {
    if (!dragRef.current) return
    const dx = e.clientX - dragRef.current.startX
    const dy = e.clientY - dragRef.current.startY
    const nx = dragRef.current.panelX + dx
    const ny = Math.max(statusBarH, dragRef.current.panelY + dy)
    movePanel(panel.id, nx, ny)
    if (tier === 'desktop' || tier === 'large') {
      setSnapPreview(getSnapZone(e.clientX, e.clientY, statusBarH))
    }
  }, [panel.id, movePanel, statusBarH, tier])

  const onTitlePointerUp = useCallback(() => {
    if (!dragRef.current) return
    if (snapPreview) snapPanel(panel.id, snapPreview)
    dragRef.current = null
    setSnapPreview(null)
  }, [snapPreview, panel.id, snapPanel])

  // ── Resize ───────────────────────────────────────────────────────────────────
  const startResize = useCallback((zone: ResizeZone, e: React.PointerEvent) => {
    e.stopPropagation()
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    resizeRef.current = {
      zone, startX: e.clientX, startY: e.clientY,
      startW: panel.w, startH: panel.h, startPX: panel.x, startPY: panel.y,
    }
    focusPanel(panel.id)
  }, [panel, focusPanel])

  const onResizePointerMove = useCallback((e: React.PointerEvent) => {
    const r = resizeRef.current
    if (!r) return
    const dx = e.clientX - r.startX
    const dy = e.clientY - r.startY
    let { startW: w, startH: h, startPX: x, startPY: y } = r
    const MIN_W = 320, MIN_H = 200
    if (r.zone.includes('e')) w = Math.max(MIN_W, r.startW + dx)
    if (r.zone.includes('s')) h = Math.max(MIN_H, r.startH + dy)
    if (r.zone.includes('w')) { w = Math.max(MIN_W, r.startW - dx); x = r.startPX + (r.startW - w) }
    if (r.zone.includes('n')) { h = Math.max(MIN_H, r.startH - dy); y = r.startPY + (r.startH - h) }
    resizePanel(panel.id, x, y, w, h)
  }, [panel.id, resizePanel])

  const onResizePointerUp = useCallback(() => { resizeRef.current = null }, [])

  // ── Double-click titlebar = toggle fullscreen ────────────────────────────────
  const onTitleDoubleClick = useCallback(() => {
    if (panel.layoutMode === 'fullscreen') {
      useAiosStore.getState().restorePanel(panel.id)
    } else {
      snapPanel(panel.id, 'fullscreen')
    }
  }, [panel.id, panel.layoutMode, snapPanel])

  if (isMinimized) return null

  const focused = isFocused && !panel.pinned

  return (
    <>
      {/* Snap preview overlay */}
      {snapPreview && createPortal(
        <SnapOverlay zone={snapPreview} statusBarH={statusBarH} dockH={dockH} />,
        document.body,
      )}

      <div
        onPointerDown={() => focusPanel(panel.id)}
        style={{
          position: 'absolute',
          ...style,
          zIndex: panel.pinned ? 9999 : panel.zIndex,
          display: 'flex', flexDirection: 'column',
          background: 'var(--bg-panel)',
          border: `1px solid ${focused ? 'var(--accent-primary)' : 'var(--border-subtle)'}`,
          borderRadius: tier === 'phone' ? 0 : 10,
          boxShadow: focused
            ? '0 0 0 1px var(--accent-primary), 0 12px 40px rgba(0,0,0,0.5)'
            : '0 8px 32px rgba(0,0,0,0.4)',
          overflow: 'hidden',
          transition: isFloating ? 'box-shadow 0.15s, border-color 0.15s' : 'all 0.2s cubic-bezier(0.4,0,0.2,1)',
          opacity: panel.pinned && !isFocused ? 0.9 : 1,
        }}
      >
        {/* Titlebar */}
        <div
          onPointerDown={onTitlePointerDown}
          onPointerMove={onTitlePointerMove}
          onPointerUp={onTitlePointerUp}
          onDoubleClick={onTitleDoubleClick}
          style={{
            display: 'flex', alignItems: 'center', gap: 8,
            height: 36, padding: '0 10px', flexShrink: 0,
            background: 'var(--bg-elevated)', borderBottom: '1px solid var(--border-subtle)',
            cursor: isFloating ? 'grab' : 'default',
            userSelect: 'none',
          }}
        >
          <span style={{ fontSize: 13, color: 'var(--text-muted)' }}>{panel.icon}</span>
          <span style={{ flex: 1, fontSize: 12, fontWeight: 600, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {panel.title}
          </span>

          {/* Split */}
          {(tier === 'desktop' || tier === 'large' || tier === 'tablet') && isFloating && (
            <TitleBtn
              icon={<PanelLeft size={11} />}
              title="Snap left / split"
              onClick={() => snapPanel(panel.id, 'left-half')}
            />
          )}

          {/* Pin */}
          <TitleBtn
            icon={<Pin size={11} style={{ color: panel.pinned ? 'var(--accent-primary)' : undefined }} />}
            title={panel.pinned ? 'Unpin' : 'Pin on top'}
            onClick={() => togglePinned(panel.id)}
          />

          {/* Fullscreen */}
          <TitleBtn
            icon={<Maximize2 size={11} />}
            title="Fullscreen"
            onClick={onTitleDoubleClick}
          />

          {/* Minimize */}
          <TitleBtn
            icon={<Minus size={11} />}
            title="Minimize"
            onClick={() => minimizePanel(panel.id)}
          />

          {/* Close */}
          <TitleBtn
            icon={<X size={11} />}
            title="Close"
            onClick={() => closePanel(panel.id)}
            danger
          />
        </div>

        {/* Content */}
        <div
          onPointerMove={resizeRef.current ? onResizePointerMove : undefined}
          onPointerUp={resizeRef.current ? onResizePointerUp : undefined}
          style={{ flex: 1, overflow: 'auto', position: 'relative', minHeight: 0 }}
        >
          {children}
        </div>

        {/* Resize handles — desktop only, floating only */}
        {isFloating && tier !== 'phone' && tier !== 'tv' && tier !== 'kiosk' && (
          <ResizeHandles onStart={startResize} />
        )}
      </div>
    </>
  )
}

function TitleBtn({ icon, title, onClick, danger }: { icon: React.ReactNode; title: string; onClick: () => void; danger?: boolean }) {
  return (
    <button
      onClick={(e) => { e.stopPropagation(); onClick() }}
      title={title}
      style={{
        width: 22, height: 22, display: 'flex', alignItems: 'center', justifyContent: 'center',
        background: 'none', border: 'none', cursor: 'pointer', borderRadius: 4,
        color: danger ? 'var(--accent-danger)' : 'var(--text-muted)',
        flexShrink: 0,
      }}
      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = 'var(--bg-panel)' }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = 'none' }}
    >
      {icon}
    </button>
  )
}

function ResizeHandles({ onStart }: { onStart: (zone: ResizeZone, e: React.PointerEvent) => void }) {
  const common: React.CSSProperties = { position: 'absolute', zIndex: 10 }
  const edge = RESIZE_HIT
  return (
    <>
      {/* Edges */}
      <div style={{ ...common, top: 0,    left: edge,  right: edge, height: edge, cursor: CURSOR.n }} onPointerDown={(e) => onStart('n', e)} />
      <div style={{ ...common, bottom: 0, left: edge,  right: edge, height: edge, cursor: CURSOR.s }} onPointerDown={(e) => onStart('s', e)} />
      <div style={{ ...common, left: 0,   top: edge,   bottom: edge, width: edge, cursor: CURSOR.w }} onPointerDown={(e) => onStart('w', e)} />
      <div style={{ ...common, right: 0,  top: edge,   bottom: edge, width: edge, cursor: CURSOR.e }} onPointerDown={(e) => onStart('e', e)} />
      {/* Corners */}
      <div style={{ ...common, top: 0,    left: 0,  width: edge, height: edge, cursor: CURSOR.nw }} onPointerDown={(e) => onStart('nw', e)} />
      <div style={{ ...common, top: 0,    right: 0, width: edge, height: edge, cursor: CURSOR.ne }} onPointerDown={(e) => onStart('ne', e)} />
      <div style={{ ...common, bottom: 0, left: 0,  width: edge, height: edge, cursor: CURSOR.sw }} onPointerDown={(e) => onStart('sw', e)} />
      <div style={{ ...common, bottom: 0, right: 0, width: edge, height: edge, cursor: CURSOR.se }} onPointerDown={(e) => onStart('se', e)} />
    </>
  )
}

function SnapOverlay({ zone, statusBarH, dockH }: { zone: SnapZone; statusBarH: number; dockH: number }) {
  const r = snapPreviewRect(zone, statusBarH, dockH)
  return (
    <div style={{
      position: 'fixed', left: r.x, top: r.y, width: r.w, height: r.h,
      background: 'var(--accent-primary)', opacity: 0.15,
      border: '2px solid var(--accent-primary)', borderRadius: 8,
      pointerEvents: 'none', zIndex: 99999,
      transition: 'all 0.1s ease',
    }} />
  )
}

// ── Layout geometry from mode ────────────────────────────────────────────────

function useLayoutStyle(panel: PanelState, statusBarH: number, dockH: number): React.CSSProperties {
  const vw = window.innerWidth
  const vh = window.innerHeight

  if (panel.layoutMode === 'floating') {
    return { left: panel.x, top: panel.y, width: panel.w, height: panel.h }
  }
  if (panel.layoutMode === 'fullscreen') {
    return { left: 0, top: statusBarH, width: vw, height: vh - statusBarH - dockH }
  }
  if (panel.layoutMode === 'sheet') {
    return { left: 0, top: statusBarH, width: vw, height: vh - statusBarH - dockH }
  }

  const canvasH = vh - statusBarH - dockH
  const halfW = vw / 2
  const halfH = canvasH / 2
  const top = statusBarH

  const map: Record<string, React.CSSProperties> = {
    'left-half':    { left: 0,     top, width: halfW, height: canvasH },
    'right-half':   { left: halfW, top, width: halfW, height: canvasH },
    'top-half':     { left: 0,     top, width: vw,    height: halfH   },
    'bottom-half':  { left: 0,     top: top + halfH, width: vw, height: halfH },
    'top-left':     { left: 0,     top, width: halfW, height: halfH },
    'top-right':    { left: halfW, top, width: halfW, height: halfH },
    'bottom-left':  { left: 0,     top: top + halfH, width: halfW, height: halfH },
    'bottom-right': { left: halfW, top: top + halfH, width: halfW, height: halfH },
  }
  return map[panel.layoutMode] ?? { left: panel.x, top: panel.y, width: panel.w, height: panel.h }
}
