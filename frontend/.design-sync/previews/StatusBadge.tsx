import { StatusBadge } from 'voidtower-frontend'

export function States() {
  return (
    <div style={{ display: 'flex', gap: 16, alignItems: 'center', flexWrap: 'wrap' }}>
      <StatusBadge state="active" />
      <StatusBadge state="activating" />
      <StatusBadge state="failed" />
      <StatusBadge state="inactive" />
    </div>
  )
}

export function WithSub() {
  return (
    <div style={{ display: 'flex', gap: 16, alignItems: 'center', flexWrap: 'wrap' }}>
      <StatusBadge state="active" sub="running" />
      <StatusBadge state="failed" sub="exit-code 1" />
    </div>
  )
}
