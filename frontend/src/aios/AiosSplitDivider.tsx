import { useRef, useCallback } from 'react'
import { useAiosStore } from '@/aios/store/aios'

export interface AiosSplitDividerProps {
  orientation?: 'vertical' | 'horizontal'
}

/**
 * Draggable divider between the two panels in a split pair.
 *
 * - Reads `splitPair` and `splitRatio` from the aios store.
 * - Pointer-drag updates `splitRatio` (clamped 0.2–0.8).
 * - Double-click resets ratio to 0.5.
 * - Vertical orientation (default): col-resize, positioned at `splitRatio * vw`.
 * - Horizontal orientation: row-resize, positioned at `splitRatio * vh`.
 */
export default function AiosSplitDivider({ orientation = 'vertical' }: AiosSplitDividerProps) {
  const { splitPair, splitRatio, setSplitRatio } = useAiosStore()
  const draggingRef = useRef(false)

  const isVertical = orientation === 'vertical'

  const onPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    draggingRef.current = true
    ;(e.currentTarget as HTMLDivElement).setPointerCapture(e.pointerId)
  }, [])

  const onPointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return
    const ratio = isVertical
      ? e.clientX / window.innerWidth
      : e.clientY / window.innerHeight
    setSplitRatio(ratio)   // store clamps to 0.2–0.8 (was 0.15–0.85 in store; use Math.max/min here too)
  }, [isVertical, setSplitRatio])

  const onPointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    draggingRef.current = false
    ;(e.currentTarget as HTMLDivElement).releasePointerCapture(e.pointerId)
  }, [])

  const onDoubleClick = useCallback(() => {
    setSplitRatio(0.5)
  }, [setSplitRatio])

  // Only render when a split pair is active
  if (!splitPair) return null

  const position = isVertical
    ? {
        position: 'fixed' as const,
        left: `calc(${splitRatio * 100}vw - 2px)`,
        top: 0,
        width: 4,
        height: '100vh',
        cursor: 'col-resize',
        flexDirection: 'column' as const,
      }
    : {
        position: 'fixed' as const,
        top: `calc(${splitRatio * 100}vh - 2px)`,
        left: 0,
        width: '100vw',
        height: 4,
        cursor: 'row-resize',
        flexDirection: 'row' as const,
      }

  return (
    <div
      style={{
        ...position,
        zIndex: 9990,
        background: 'rgba(255,255,255,0.1)',
        touchAction: 'none',
        transition: draggingRef.current ? undefined : 'left 0.05s, top 0.05s',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
      }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onDoubleClick={onDoubleClick}
      onMouseEnter={(e) => { (e.currentTarget as HTMLDivElement).style.background = 'var(--accent-primary, #6366f1)' }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLDivElement).style.background = 'rgba(255,255,255,0.1)' }}
    >
      {/* Visual grip dots */}
      <div
        style={{
          display: 'flex',
          flexDirection: isVertical ? 'column' : 'row',
          gap: 3,
          pointerEvents: 'none',
        }}
      >
        {[0, 1, 2].map((i) => (
          <div
            key={i}
            style={{
              width: isVertical ? 2 : 4,
              height: isVertical ? 4 : 2,
              borderRadius: 1,
              background: 'rgba(255,255,255,0.4)',
            }}
          />
        ))}
      </div>
    </div>
  )
}
