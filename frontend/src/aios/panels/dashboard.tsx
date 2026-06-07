import { useEffect, useState } from 'react'
import NativePanelShell, { NativeRow, StatusDot, EmptyState, LoadingState } from './NativePanelShell'

interface Proc { name: string; cpu_usage: number; memory_bytes: number }
interface GPU  { name: string; util_pct: number; temp_c: number }
interface Snapshot {
  cpu_usage: number; ram_used: number; ram_total: number
  disks: Array<{ mount_point: string; used: number; total: number }>
  networks: Array<{ name: string; rx_bytes_per_sec: number; tx_bytes_per_sec: number }>
  top_cpu_procs?: Proc[]
  gpu?: GPU[]
  hostname?: string
  uptime_secs?: number
}

function Bar({ pct, color = 'var(--accent-primary)' }: { pct: number; color?: string }) {
  return (
    <div style={{ flex: 1, height: 4, background: 'var(--bg-elevated)', borderRadius: 2, overflow: 'hidden' }}>
      <div style={{ width: `${Math.min(100, pct)}%`, height: '100%', background: color, borderRadius: 2, transition: 'width 0.3s' }} />
    </div>
  )
}

function fmt(b: number) {
  if (b >= 1e9) return (b / 1e9).toFixed(1) + ' GB'
  if (b >= 1e6) return (b / 1e6).toFixed(0) + ' MB'
  return (b / 1e3).toFixed(0) + ' KB'
}

function fmtRate(bps: number) {
  if (bps >= 1e6) return (bps / 1e6).toFixed(1) + ' MB/s'
  if (bps >= 1e3) return (bps / 1e3).toFixed(0) + ' KB/s'
  return bps.toFixed(0) + ' B/s'
}

function fmtUptime(s: number) {
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600)
  return d > 0 ? `${d}d ${h}h` : `${h}h ${Math.floor((s % 3600) / 60)}m`
}

function SL({ text }: { text: string }) {
  return <div style={{ padding: '6px 10px 3px', fontSize: 10, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.06em' }}>{text}</div>
}

export default function NativeDashboardPanel() {
  const [snap, setSnap] = useState<Snapshot | null>(null)
  const [loading, setLoading] = useState(true)

  async function load() {
    const r = await fetch('/api/metrics/current', { credentials: 'include' })
    if (r.ok) setSnap(await r.json())
    setLoading(false)
  }

  useEffect(() => { load(); const t = setInterval(load, 5000); return () => clearInterval(t) }, [])

  if (loading) return <NativePanelShell><LoadingState /></NativePanelShell>
  if (!snap)   return <NativePanelShell><EmptyState text="No metrics" /></NativePanelShell>

  const cpu = snap.cpu_usage
  const memPct = (snap.ram_used / snap.ram_total) * 100
  const disk0 = snap.disks?.[0]
  const diskPct = disk0 ? (disk0.used / disk0.total) * 100 : 0
  const net = snap.networks?.find(n => !n.name.startsWith('lo'))
  const procs = snap.top_cpu_procs?.slice(0, 3) ?? []
  const gpus  = snap.gpu ?? []

  return (
    <NativePanelShell>
      {(snap.hostname || snap.uptime_secs !== undefined) && (
        <NativeRow>
          <span style={{ flex: 1, fontSize: 11, color: 'var(--text-primary)', fontWeight: 500 }}>{snap.hostname ?? '—'}</span>
          {snap.uptime_secs !== undefined && (
            <span style={{ fontSize: 9, color: 'var(--text-muted)' }}>up {fmtUptime(snap.uptime_secs)}</span>
          )}
        </NativeRow>
      )}
      <SL text="Resources" />
      <NativeRow>
        <span style={{ width: 28, fontSize: 10, color: 'var(--text-muted)' }}>CPU</span>
        <Bar pct={cpu} color={cpu > 85 ? '#ef4444' : cpu > 65 ? '#f59e0b' : 'var(--accent-primary)'} />
        <span style={{ width: 34, fontSize: 10, color: 'var(--text-secondary)', textAlign: 'right' }}>{cpu.toFixed(0)}%</span>
      </NativeRow>
      <NativeRow>
        <span style={{ width: 28, fontSize: 10, color: 'var(--text-muted)' }}>RAM</span>
        <Bar pct={memPct} color={memPct > 90 ? '#ef4444' : 'var(--accent-primary)'} />
        <span style={{ width: 34, fontSize: 10, color: 'var(--text-secondary)', textAlign: 'right' }}>{memPct.toFixed(0)}%</span>
      </NativeRow>
      {disk0 && (
        <NativeRow>
          <span style={{ width: 28, fontSize: 10, color: 'var(--text-muted)' }}>DSK</span>
          <Bar pct={diskPct} color={diskPct > 90 ? '#ef4444' : diskPct > 75 ? '#f59e0b' : 'var(--accent-primary)'} />
          <span style={{ width: 34, fontSize: 10, color: 'var(--text-secondary)', textAlign: 'right' }}>{diskPct.toFixed(0)}%</span>
        </NativeRow>
      )}
      {net && (
        <NativeRow>
          <span style={{ width: 28, fontSize: 10, color: 'var(--text-muted)' }}>NET</span>
          <span style={{ flex: 1, fontSize: 10, color: 'var(--text-secondary)' }}>↓{fmtRate(net.rx_bytes_per_sec)} ↑{fmtRate(net.tx_bytes_per_sec)}</span>
        </NativeRow>
      )}
      <NativeRow>
        <span style={{ flex: 1, fontSize: 10, color: 'var(--text-muted)' }}>RAM used</span>
        <span style={{ fontSize: 10, color: 'var(--text-secondary)' }}>{fmt(snap.ram_used)} / {fmt(snap.ram_total)}</span>
      </NativeRow>
      {procs.length > 0 && (
        <>
          <SL text="Top CPU" />
          {procs.map(p => (
            <NativeRow key={p.name}>
              <StatusDot color="#94a3b8" />
              <span style={{ flex: 1, fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{p.name}</span>
              <span style={{ fontSize: 10, color: 'var(--text-secondary)', fontVariantNumeric: 'tabular-nums' }}>{p.cpu_usage.toFixed(1)}%</span>
            </NativeRow>
          ))}
        </>
      )}
      {gpus.length > 0 && (
        <>
          <SL text="GPU" />
          {gpus.map((g, i) => (
            <NativeRow key={i}>
              <StatusDot color={g.temp_c >= 85 ? '#ef4444' : g.temp_c >= 70 ? '#f59e0b' : '#22c55e'} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{g.name}</div>
                <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{g.util_pct.toFixed(0)}% util · {g.temp_c.toFixed(0)}°C</div>
              </div>
            </NativeRow>
          ))}
        </>
      )}
    </NativePanelShell>
  )
}
