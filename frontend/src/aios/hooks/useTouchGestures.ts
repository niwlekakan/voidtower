import { useEffect, useRef, type RefObject } from 'react'

export interface TouchGestureHandlers {
  onLongPress?: () => void
  onSwipeDown?: () => void
  onSwipeLeft?: () => void
  onSwipeRight?: () => void
  /** positive = expand, negative = shrink */
  onPinchResize?: (delta: number) => void
}

const LONG_PRESS_MS   = 500
const LONG_PRESS_SLOP = 8    // px — movement beyond this cancels long-press
const SWIPE_MIN_PX    = 80   // minimum displacement to register a swipe
const SWIPE_MAX_MS    = 400  // swipe must complete within this window

function touchDistance(a: Touch, b: Touch): number {
  const dx = a.clientX - b.clientX
  const dy = a.clientY - b.clientY
  return Math.sqrt(dx * dx + dy * dy)
}

/**
 * Attaches pointer-event based gesture detection (long-press, swipe) and
 * touch-event based pinch detection to the element referenced by `ref`.
 *
 * All listeners are removed on unmount.
 */
export function useTouchGestures(
  ref: RefObject<HTMLElement>,
  handlers: TouchGestureHandlers,
): void {
  // Keep handlers in a ref so the effect closure is always fresh without
  // needing to re-attach listeners on every render.
  const handlersRef = useRef<TouchGestureHandlers>(handlers)
  useEffect(() => {
    handlersRef.current = handlers
  })

  useEffect(() => {
    const el = ref.current
    if (!el) return

    // ------------------------------------------------------------------
    // Pointer-event state (long-press + swipe)
    // ------------------------------------------------------------------
    let downX = 0
    let downY = 0
    let downTime = 0
    let longPressTimer: ReturnType<typeof setTimeout> | null = null
    let gestureDone = false  // prevents double-firing on pointerup after long-press

    function clearLongPress() {
      if (longPressTimer !== null) {
        clearTimeout(longPressTimer)
        longPressTimer = null
      }
    }

    function onPointerDown(e: PointerEvent) {
      // Only track primary pointer (ignore secondary touches for pointer events)
      if (!e.isPrimary) return
      downX = e.clientX
      downY = e.clientY
      downTime = e.timeStamp
      gestureDone = false

      longPressTimer = setTimeout(() => {
        longPressTimer = null
        gestureDone = true
        handlersRef.current.onLongPress?.()
      }, LONG_PRESS_MS)
    }

    function onPointerMove(e: PointerEvent) {
      if (!e.isPrimary) return
      const dx = e.clientX - downX
      const dy = e.clientY - downY
      const dist = Math.sqrt(dx * dx + dy * dy)
      if (dist > LONG_PRESS_SLOP) {
        clearLongPress()
      }
    }

    function onPointerUp(e: PointerEvent) {
      if (!e.isPrimary) return
      clearLongPress()
      if (gestureDone) return  // long-press already fired

      const dx = e.clientX - downX
      const dy = e.clientY - downY
      const dt = e.timeStamp - downTime
      if (dt > SWIPE_MAX_MS) return

      const absDx = Math.abs(dx)
      const absDy = Math.abs(dy)

      if (absDy > SWIPE_MIN_PX && absDy > absDx) {
        if (dy > 0) handlersRef.current.onSwipeDown?.()
        // (swipe-up is not in the spec; ignore)
      } else if (absDx > SWIPE_MIN_PX && absDx > absDy) {
        if (dx < 0) handlersRef.current.onSwipeLeft?.()
        else        handlersRef.current.onSwipeRight?.()
      }
    }

    function onPointerCancel(e: PointerEvent) {
      if (!e.isPrimary) return
      clearLongPress()
    }

    el.addEventListener('pointerdown',   onPointerDown)
    el.addEventListener('pointermove',   onPointerMove)
    el.addEventListener('pointerup',     onPointerUp)
    el.addEventListener('pointercancel', onPointerCancel)

    // ------------------------------------------------------------------
    // Touch events — pinch (2-finger)
    // ------------------------------------------------------------------
    let initialPinchDist = 0
    let pinchActive = false

    function onTouchStart(e: TouchEvent) {
      if (e.touches.length === 2) {
        initialPinchDist = touchDistance(e.touches[0], e.touches[1])
        pinchActive = true
        // Suppress pointer events for this gesture so they don't fire swipe
        clearLongPress()
      }
    }

    function onTouchMove(e: TouchEvent) {
      if (!pinchActive || e.touches.length !== 2) return
      const newDist = touchDistance(e.touches[0], e.touches[1])
      const delta = newDist - initialPinchDist
      handlersRef.current.onPinchResize?.(delta)
    }

    function onTouchEnd(e: TouchEvent) {
      if (e.touches.length < 2) {
        pinchActive = false
        initialPinchDist = 0
      }
    }

    function onTouchCancel() {
      pinchActive = false
      initialPinchDist = 0
    }

    el.addEventListener('touchstart',  onTouchStart,  { passive: true })
    el.addEventListener('touchmove',   onTouchMove,   { passive: true })
    el.addEventListener('touchend',    onTouchEnd,    { passive: true })
    el.addEventListener('touchcancel', onTouchCancel, { passive: true })

    return () => {
      clearLongPress()
      el.removeEventListener('pointerdown',   onPointerDown)
      el.removeEventListener('pointermove',   onPointerMove)
      el.removeEventListener('pointerup',     onPointerUp)
      el.removeEventListener('pointercancel', onPointerCancel)
      el.removeEventListener('touchstart',    onTouchStart)
      el.removeEventListener('touchmove',     onTouchMove)
      el.removeEventListener('touchend',      onTouchEnd)
      el.removeEventListener('touchcancel',   onTouchCancel)
    }
  }, [ref])
}
