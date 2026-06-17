import { MetricCard } from 'voidtower-frontend'

export function Default() {
  return (
    <div style={{ width: 220 }}>
      <MetricCard label="Uptime" value="14d 6h" />
    </div>
  )
}

export function Accent() {
  return (
    <div style={{ width: 220 }}>
      <MetricCard label="CPU" value="42.7%" sub="Ryzen 7 5800X" accent />
    </div>
  )
}

export function Dashboard() {
  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 10 }}>
      <MetricCard label="CPU" value="42.7%" sub="Ryzen 7 5800X" accent />
      <MetricCard label="RAM" value="61.3%" sub="9.8 GB / 16 GB" />
      <MetricCard label="Uptime" value="14d 6h" />
      <MetricCard label="Processes" value={284} />
    </div>
  )
}
