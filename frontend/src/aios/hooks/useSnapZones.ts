import { useState, useCallback } from 'react'
import type { LayoutMode } from '../store/aios'
import type { SnapZone as StoreSnapZone } from '../store/aios'

// ---- SnapPreview (legacy utility) ----------------------------------------

export interface SnapPreview {
  zone: StoreSnapZone
  rect: { x: number; y: number; w: number; h: number }
}

const EDGE_PX   = 80   // pixels from edge to trigger side snaps
const TOP_PX    = 40   // pixels from top to trigger fullscreen
const CORNER_PX = 80   // pixels from corner edge (both axes)

export function getSnapZone(
  cursorX: number,
  cursorY: number,
  statusBarH = 28,
): StoreSnapZone | null {
  const vw = window.innerWidth
  const vh = window.innerHeight
  const canvasTop = statusBarH

  const nearLeft   = cursorX < EDGE_PX
  const nearRight  = cursorX > vw - EDGE_PX
  const nearTop    = cursorY < canvasTop + TOP_PX
  const nearBottom = cursorY > vh - CORNER_PX

  // Fullscreen: drag near top edge (not a corner)
  if (nearTop && !nearLeft && !nearRight) return 'fullscreen'

  // Corners
  if (nearLeft  && nearTop)    return 'top-left'
  if (nearRight && nearTop)    return 'top-right'
  if (nearLeft  && nearBottom) return 'bottom-left'
  if (nearRight && nearBottom) return 'bottom-right'

  // Halves
  if (nearLeft)  return 'left-half'
  if (nearRight) return 'right-half'

  return null
}

export function snapPreviewRect(
  zone: StoreSnapZone,
  statusBarH = 28,
  dockH = 56,
): { x: number; y: number; w: number; h: number } {
  const vw = window.innerWidth
  const vh = window.innerHeight
  const canvasH = vh - statusBarH - dockH
  const halfW = vw / 2
  const halfH = canvasH / 2
  const top = statusBarH

  switch (zone) {
    case 'left-half':    return { x: 0,     y: top,         w: halfW, h: canvasH }
    case 'right-half':   return { x: halfW, y: top,         w: halfW, h: canvasH }
    case 'top-half':     return { x: 0,     y: top,         w: vw,    h: halfH   }
    case 'bottom-half':  return { x: 0,     y: top + halfH, w: vw,    h: halfH   }
    case 'top-left':     return { x: 0,     y: top,         w: halfW, h: halfH   }
    case 'top-right':    return { x: halfW, y: top,         w: halfW, h: halfH   }
    case 'bottom-left':  return { x: 0,     y: top + halfH, w: halfW, h: halfH   }
    case 'bottom-right': return { x: halfW, y: top + halfH, w: halfW, h: halfH   }
    case 'fullscreen':   return { x: 0,     y: top,         w: vw,    h: canvasH }
  }
}

// ---- useSnapZones hook -------------------------------------------------------

/** A snap zone with both the layout mode and the preview rect in px. */
export interface SnapZone {
  mode: LayoutMode
  rect: { x: number; y: number; w: number; h: number }
}

/** Threshold distances for the hook-level API */
const HOOK_EDGE_PX   = 32
const HOOK_CORNER_PX = 64

function computeZone(
  px: number,
  py: number,
  canvasW: number,
  canvasH: number,
): SnapZone | null {
  const halfW = canvasW / 2
  const halfH = canvasH / 2

  const nearLeft   = px <= HOOK_EDGE_PX
  const nearRight  = px >= canvasW - HOOK_EDGE_PX
  const nearTop    = py <= HOOK_EDGE_PX
  const nearBottom = py >= canvasH - HOOK_EDGE_PX

  const nearLeftCorner   = px <= HOOK_CORNER_PX
  const nearRightCorner  = px >= canvasW - HOOK_CORNER_PX
  const nearTopCorner    = py <= HOOK_CORNER_PX
  const nearBottomCorner = py >= canvasH - HOOK_CORNER_PX

  // Corners take priority over edges
  if (nearLeftCorner  && nearTopCorner)    return { mode: 'top-left',     rect: { x: 0,     y: 0,     w: halfW,  h: halfH  } }
  if (nearRightCorner && nearTopCorner)    return { mode: 'top-right',    rect: { x: halfW, y: 0,     w: halfW,  h: halfH  } }
  if (nearLeftCorner  && nearBottomCorner) return { mode: 'bottom-left',  rect: { x: 0,     y: halfH, w: halfW,  h: halfH  } }
  if (nearRightCorner && nearBottomCorner) return { mode: 'bottom-right', rect: { x: halfW, y: halfH, w: halfW,  h: halfH  } }

  // Center-top → fullscreen: within top HOOK_EDGE_PX AND center 33% of width
  const centerLeft  = canvasW / 3
  const centerRight = (canvasW * 2) / 3
  if (nearTop && px >= centerLeft && px <= centerRight) {
    return { mode: 'fullscreen', rect: { x: 0, y: 0, w: canvasW, h: canvasH } }
  }

  // Cardinal edges
  if (nearLeft)   return { mode: 'left-half',   rect: { x: 0,     y: 0, w: halfW,  h: canvasH } }
  if (nearRight)  return { mode: 'right-half',  rect: { x: halfW, y: 0, w: halfW,  h: canvasH } }
  if (nearTop)    return { mode: 'top-half',    rect: { x: 0,     y: 0, w: canvasW, h: halfH  } }
  if (nearBottom) return { mode: 'bottom-half', rect: { x: 0,     y: halfH, w: canvasW, h: halfH } }

  return null
}

/**
 * Tracks the currently hovered snap zone during a drag.
 *
 * Call `getSnapForPosition` with the current pointer position and the canvas
 * dimensions to update `activeZone`. The returned zone includes the layout
 * mode and a pixel-space preview rect for rendering a drop-indicator overlay.
 */
export function useSnapZones(): {
  activeZone: SnapZone | null
  getSnapForPosition: (px: number, py: number, canvasW: number, canvasH: number) => SnapZone | null
} {
  const [activeZone, setActiveZone] = useState<SnapZone | null>(null)

  const getSnapForPosition = useCallback(
    (px: number, py: number, canvasW: number, canvasH: number): SnapZone | null => {
      const zone = computeZone(px, py, canvasW, canvasH)
      setActiveZone(zone)
      return zone
    },
    [],
  )

  return { activeZone, getSnapForPosition }
}
