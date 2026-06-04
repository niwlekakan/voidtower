interface MetricCardProps {
  label: string
  value: string | number
  sub?: string
  accent?: boolean
}

export default function MetricCard({ label, value, sub, accent }: MetricCardProps) {
  return (
    <div className="card flex flex-col gap-1">
      <div className="text-xs uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{label}</div>
      <div
        className="text-2xl font-semibold font-mono"
        style={{ color: accent ? 'var(--accent-primary)' : 'var(--text-primary)' }}
      >
        {value}
      </div>
      {sub && <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>{sub}</div>}
    </div>
  )
}
