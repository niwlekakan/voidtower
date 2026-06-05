import { useState, useEffect } from 'react'
import { type DeviceTier, useAiosStore } from '../store/aios'

export type { DeviceTier }

const OVERRIDE_KEY = 'vt-device-mode'

function detect(): DeviceTier {
  if (typeof window === 'undefined') return 'desktop'

  // 1. Explicit override from localStorage
  const override = localStorage.getItem(OVERRIDE_KEY) as DeviceTier | null
  if (override) return override

  // 5. Kiosk via query param
  const params = new URLSearchParams(window.location.search)
  if (params.get('mode') === 'kiosk') return 'kiosk'

  const w = window.innerWidth
  const coarsePointer = window.matchMedia('(pointer: coarse)').matches
  const noHover = window.matchMedia('(hover: none)').matches

  // 2. Touch device check
  // 4. TV: coarse + large screen
  if (coarsePointer && noHover && w >= 1200) return 'tv'

  // 3. Breakpoints
  if (w < 640) return 'phone'
  if (w < 1200) return 'tablet'
  if (w >= 1920) return 'large'
  return 'desktop'
}

export function useDeviceTier(): DeviceTier {
  const [tier, setTier] = useState<DeviceTier>(detect)
  const setStoreTier = useAiosStore((s) => s.setDeviceTier)

  useEffect(() => {
    const update = () => {
      const next = detect()
      setTier(next)
      setStoreTier(next)
    }

    // Sync store with initial detected value
    setStoreTier(tier)

    window.addEventListener('resize', update)

    // Listen for media query changes (pointer/hover capability changes)
    const pointerMq = window.matchMedia('(pointer: coarse)')
    const hoverMq = window.matchMedia('(hover: none)')
    pointerMq.addEventListener('change', update)
    hoverMq.addEventListener('change', update)

    return () => {
      window.removeEventListener('resize', update)
      pointerMq.removeEventListener('change', update)
      hoverMq.removeEventListener('change', update)
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  return tier
}

export function setDeviceTierOverride(tier: DeviceTier | null) {
  if (tier) localStorage.setItem(OVERRIDE_KEY, tier)
  else localStorage.removeItem(OVERRIDE_KEY)
}
