import { MetricChart } from 'voidtower-frontend'

const cpuHistory = [12, 18, 15, 22, 31, 28, 35, 42, 38, 45, 40, 48, 52, 47, 41].map((v, t) => ({ t, v }))
const ramHistory = [58, 59, 61, 60, 62, 64, 63, 65, 66, 64, 67, 68, 66, 69, 70].map((v, t) => ({ t, v }))

export function CpuUsage() {
  return (
    <div style={{ width: 320, height: 100 }} className="card">
      <MetricChart data={cpuHistory} unit="%" color="var(--accent-primary)" />
    </div>
  )
}

export function RamUsage() {
  return (
    <div style={{ width: 320, height: 100 }} className="card">
      <MetricChart data={ramHistory} unit="%" color="var(--accent-secondary)" />
    </div>
  )
}

export function TallVariant() {
  return (
    <div style={{ width: 320, height: 160 }} className="card">
      <MetricChart data={cpuHistory} unit="%" color="var(--accent-success)" height={140} />
    </div>
  )
}
