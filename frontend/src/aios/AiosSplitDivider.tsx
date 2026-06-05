import { useCallback, useRef } from 'react'
import { useAiosStore } from '@/aios/store/aios'

interface Props { statusBarH: number; dockH: number }

export default function AiosSplitDivider({ statusBarH, dockH }: Props) {
  const { splitRatio, setSplitRatio } = useAiosStore()
  const dragging = useRef(false)

  const onPointerDown = useCallback((e: React.PointerEvent) => {
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    dragging.current = true
  }, [])

  const onPointerMove = useCallback((e: React.PointerEvent) => {
    if (!dragging.current) return
    setSplitRatio(e.clientX / window.innerWidth)
  }, [setSplitRatio])

  const onPointerUp = useCallback(() => { dragging.current = false }, [])

  const onDoubleClick = useCallback(() => setSplitRatio(0.5), [setSplitRatio])

  const x = window.innerWidth * splitRatio
  const top = statusBarH
  const height = window.innerHeight - statusBarH - dockH

  return (
    <div
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onDoubleClick={onDoubleClick}
      style={{
        position: 'fixed', left: x - 2, top, width: 4, height,
        cursor: 'col-resize', zIndex: 9990,
        background: 'var(--border-subtle)',
        transition: 'background 0.15s',
      }}
      onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = 'var(--accent-primary)' }}
      onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = 'var(--border-subtle)' }}
    />
  )
}
