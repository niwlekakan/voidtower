import type { SnapZone } from '@/aios/store/aios'

export interface SnapPreview {
  zone: SnapZone
  rect: { x: number; y: number; w: number; h: number }
}

const EDGE_PX = 80    // pixels from edge to trigger side snaps
const TOP_PX  = 40    // pixels from top to trigger fullscreen
const CORNER_PX = 80  // pixels from corner edge (both axes)

export function getSnapZone(
  cursorX: number,
  cursorY: number,
  statusBarH = 28,
): SnapZone | null {
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
  zone: SnapZone,
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
    case 'left-half':    return { x: 0,     y: top,           w: halfW, h: canvasH }
    case 'right-half':   return { x: halfW, y: top,           w: halfW, h: canvasH }
    case 'top-half':     return { x: 0,     y: top,           w: vw,    h: halfH   }
    case 'bottom-half':  return { x: 0,     y: top + halfH,   w: vw,    h: halfH   }
    case 'top-left':     return { x: 0,     y: top,           w: halfW, h: halfH   }
    case 'top-right':    return { x: halfW, y: top,           w: halfW, h: halfH   }
    case 'bottom-left':  return { x: 0,     y: top + halfH,   w: halfW, h: halfH   }
    case 'bottom-right': return { x: halfW, y: top + halfH,   w: halfW, h: halfH   }
    case 'fullscreen':   return { x: 0,     y: top,           w: vw,    h: canvasH }
  }
}
