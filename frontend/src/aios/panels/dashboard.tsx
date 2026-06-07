import { useEffect, useState } from 'react'
import NativePanelShell, { NativeRow, StatusDot, EmptyState, LoadingState } from './NativePanelShell'

interface Snapshot {
  cpu: { usage_percent: number }
  memory: { used_bytes: number; total_bytes: number }
  disks: Array<{ mount: string; used_bytes: number; total_bytes: number }>
  network: Array<{ interface: string; rx_bytes_sec: number; tx_bytes_sec: number }>
}

interface Summary { services: number; containers: number; alerts: number }

function Bar({ pct, color = 'var(--accent-primary)' }: { pct: number; color?: string }) {
  return (
    <div style={{ flex: 1, height: 4, background: 'var(--bg-elevated)', borderRadius: 2, overflow: 'hidden' }}>
      <div style={{ width: `${Math.min(100, pct)}%`, height: '100%', background: color, borderRadius: 2, transition: 'width 0.3s' }} />
    </div>
  )
}

function fmt(bytes: number) {
  if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + ' GB'
  if (bytes >= 1e6) return (bytes / 1e6).toFixed(0) + ' MB'
  return (bytes / 1e3).toFixed(0) + ' KB'
}

function fmtRate(bps: number) {
  if (bps >= 1e6) return (bps / 1e6).toFixed(1) + ' MB/s'
  if (bps >= 1e3) return (bps / 1e3).toFixed(0) + ' KB/s'
  return bps.toFixed(0) + ' B/s'
}

function SL({ text }: { text: string }) {
  return <div style={{ padding: '6px 10px 3px', fontSize: 10, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>{text}</div>
}

export default function NativeDashboardPanel() {
  const [snap, setSnap] = useState<Snapshot | null>(null)
  const [summary, setSummary] = useState<Summary | null>(null)
  const [loading, setLoading] = useState(true)

  async function load() {
    const [m, s, c, a] = await Promise.allSettled([
      fetch('/api/metrics/current', { credentials: 'include' }).then(r => r.json()),
      fetch('/api/services', { credentials: 'include' }).then(r => r.json()),
      fetch('/api/containers', { credentials: 'include' }).then(r => r.json()),
      fetch('/api/alerts?state=active', { credentials: 'include' }).then(r => r.json()),
    ])
    if (m.status === 'fulfilled') setSnap(m.value)
    setSummary({
      services:   s.status === 'fulfilled' ? (s.value.services?.length ?? 0) : 0,
      containers: c.status === 'fulfilled' ? (c.value.containers?.length ?? 0) : 0,
      alerts:     a.status === 'fulfilled' ? (a.value.alerts?.length ?? 0) : 0,
    })
    setLoading(false)
  }

  useEffect(() => { load(); const t = setInterval(load, 5000); return () => clearInterval(t) }, [])

  if (loading) return <NativePanelShell><LoadingState /></NativePanelShell>

  const cpu = snap?.cpu.usage_percent ?? 0
  const memPct = snap ? (snap.memory.used_bytes / snap.memory.total_bytes) * 100 : 0
  const disk0 = snap?.disks?.[0]
  const diskPct = disk0 ? (disk0.used_bytes / disk0.total_bytes) * 100 : 0
  const net = snap?.network?.find(n => !n.interface.startsWith('lo')) ?? null

  return (
    <NativePanelShell>
      {!snap ? <EmptyState text="No metrics" /> : (
        <>
          <SL text="Resources" />
          <NativeRow>
            <span style={{ width: 28, fontSize: 10, color: 'var(--text-muted)' }}>CPU</span>
            <Bar pct={cpu} color={cpu > 85 ? 'var(--accent-error, #ef4444)' : cpu > 65 ? 'var(--accent-warning, #f59e0b)' : 'var(--accent-primary)'} />
            <span style={{ width: 34, fontSize: 10, color: 'var(--text-secondary)', textAlign: 'right' }}>{cpu.toFixed(0)}%</span>
          </NativeRow>
          <NativeRow>
            <span style={{ width: 28, fontSize: 10, color: 'var(--text-muted)' }}>RAM</span>
            <Bar pct={memPct} color={memPct > 90 ? 'var(--accent-error, #ef4444)' : 'var(--accent-primary)'} />
            <span style={{ width: 34, fontSize: 10, color: 'var(--text-secondary)', textAlign: 'right' }}>{memPct.toFixed(0)}%</span>
          </NativeRow>
          {disk0 && (
            <NativeRow>
              <span style={{ width: 28, fontSize: 10, color: 'var(--text-muted)' }}>DSK</span>
              <Bar pct={diskPct} color={diskPct > 90 ? 'var(--accent-error, #ef4444)' : 'var(--accent-primary)'} />
              <span style={{ width: 34, fontSize: 10, color: 'var(--text-secondary)', textAlign: 'right' }}>{diskPct.toFixed(0)}%</span>
            </NativeRow>
          )}
          {net && (
            <NativeRow>
              <span style={{ width: 28, fontSize: 10, color: 'var(--text-muted)' }}>NET</span>
              <span style={{ flex: 1, fontSize: 10, color: 'var(--text-secondary)' }}>↓{fmtRate(net.rx_bytes_sec)} ↑{fmtRate(net.tx_bytes_sec)}</span>
            </NativeRow>
          )}
          {summary && (
            <>
              <SL text="Overview" />
              {[
                { label: 'Services', val: summary.services, color: '#22c55e' },
                { label: 'Containers', val: summary.containers, color: '#3b82f6' },
                { label: 'Active Alerts', val: summary.alerts, color: summary.alerts > 0 ? '#ef4444' : '#22c55e' },
              ].map(({ label, val, color }) => (
                <NativeRow key={label}>
                  <StatusDot color={color} />
                  <span style={{ flex: 1, fontSize: 11, color: 'var(--text-primary)' }}>{label}</span>
                  <span style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-secondary)' }}>{val}</span>
                </NativeRow>
              ))}
            </>
          )}
          <SL text="Memory" />
          <NativeRow>
            <span style={{ flex: 1, fontSize: 10, color: 'var(--text-muted)' }}>Used / Total</span>
            <span style={{ fontSize: 10, color: 'var(--text-secondary)' }}>{fmt(snap.memory.used_bytes)} / {fmt(snap.memory.total_bytes)}</span>
          </NativeRow>
        </>
      )}
    </NativePanelShell>
  )
}
