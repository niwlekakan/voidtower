interface StatusBadgeProps {
  state: string
  sub?: string
}

function stateColor(state: string): string {
  switch (state.toLowerCase()) {
    case 'active':   return 'var(--accent-success)'
    case 'failed':   return 'var(--accent-danger)'
    case 'inactive': return 'var(--text-muted)'
    default:         return 'var(--accent-warning)'
  }
}

export default function StatusBadge({ state, sub }: StatusBadgeProps) {
  const color = stateColor(state)
  return (
    <span className="inline-flex items-center gap-1.5 text-xs">
      <span className="status-dot" style={{ background: color, boxShadow: state.toLowerCase() === 'active' ? `0 0 6px ${color}` : undefined }} />
      <span style={{ color }}>{state}{sub ? ` (${sub})` : ''}</span>
    </span>
  )
}
