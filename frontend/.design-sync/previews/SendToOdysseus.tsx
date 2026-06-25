import type { CSSProperties } from 'react'
import { SendToOdysseus } from 'voidtower-frontend'

// Low-contrast accent tones assume the app's dark surface — see AiBadge's
// preview for the same gotcha (NOTES.md).
const surface: CSSProperties = { background: 'var(--bg-card)', padding: 12, borderRadius: 6, display: 'inline-block' }

export function IconOnly() {
  return (
    <div style={surface}>
      <SendToOdysseus context="proxy config for app.example.com" />
    </div>
  )
}

export function Labeled() {
  return (
    <div style={surface}>
      <SendToOdysseus context="container logs for gitea" label="Send to Odysseus" />
    </div>
  )
}
