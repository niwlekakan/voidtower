import { useState, useEffect } from 'react'

export type DeviceTier = 'phone' | 'tablet' | 'desktop' | 'large' | 'tv' | 'kiosk'

const KIOSK_PARAM = 'mode'

function detect(): DeviceTier {
  if (typeof window === 'undefined') return 'desktop'

  // Explicit overrides
  const override = localStorage.getItem('vt-device-tier') as DeviceTier | null
  if (override) return override

  const params = new URLSearchParams(window.location.search)
  if (params.get(KIOSK_PARAM) === 'kiosk') return 'kiosk'

  const w = window.innerWidth
  const coarsePointer = window.matchMedia('(pointer: coarse)').matches
  const noHover = window.matchMedia('(hover: none)').matches

  // TV: large screen + coarse pointer (or TV user-agent hints)
  if (coarsePointer && noHover && w >= 1200) return 'tv'

  if (w < 640) return 'phone'
  if (w < 1200) return 'tablet'
  if (w >= 1920) return 'large'
  return 'desktop'
}

export function useDeviceTier(): DeviceTier {
  const [tier, setTier] = useState<DeviceTier>(detect)

  useEffect(() => {
    const onResize = () => setTier(detect())
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [])

  return tier
}

export function setDeviceTierOverride(tier: DeviceTier | null) {
  if (tier) localStorage.setItem('vt-device-tier', tier)
  else localStorage.removeItem('vt-device-tier')
}
