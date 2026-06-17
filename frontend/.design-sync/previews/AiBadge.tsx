import type { CSSProperties } from 'react'
import { AiBadge } from 'voidtower-frontend'

// AiBadge's "ready" level uses translucent near-white tones meant to sit on
// the app's dark surface (var(--bg-card)) — render on that, not page-white,
// or the lowest-emphasis level is invisible.
const surface: CSSProperties = { background: 'var(--bg-card)', padding: 12, borderRadius: 6 }

export function Levels() {
  return (
    <div style={{ ...surface, display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
      <AiBadge level="native" description="Built for AI from the ground up" />
      <AiBadge level="aware" description="Exposes hooks for AI integration" />
      <AiBadge level="ready" description="Can be wired up with extra setup" />
    </div>
  )
}

export function Compact() {
  return (
    <div style={{ ...surface, display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
      <AiBadge level="native" compact />
      <AiBadge level="aware" compact />
      <AiBadge level="ready" compact />
    </div>
  )
}
