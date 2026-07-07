import { useCallback, useEffect, useState } from 'react'
import NativePanelShell, { NativeRow, StatusDot, EmptyState, LoadingState } from './NativePanelShell'

interface StorageDevice { name: string; size: number; type?: string; model?: string; fs_type?: string }
interface StorageMount  { device: string; mountpoint: string; fstype: string; total: number; used: number; available: number; use_pct: number }
interface RaidArray     { name: string; level: string; state: string; members: string[] }

const TABS = [
  { id: 'devices', label: 'Devices' },
  { id: 'mounts',  label: 'Mounts'  },
  { id: 'raid',    label: 'RAID'    },
]

function fmt(b: number) {
  if (b > 1e12) return `${(b / 1e12).toFixed(1)}T`
  if (b > 1e9)  return `${(b / 1e9).toFixed(1)}G`
  return `${(b / 1e6).toFixed(0)}M`
}

function barColor(p: number) {
  return p > 90 ? '#ef4444' : p > 75 ? '#f59e0b' : 'var(--accent-primary)'
}

function pctDot(p: number) {
  return p > 90 ? '#ef4444' : p > 75 ? '#f59e0b' : '#22c55e'
}

function raidDot(state: string) {
  const s = state.toLowerCase()
  if (s.includes('active') || s.includes('clean')) return '#22c55e'
  if (s.includes('degraded')) return '#f59e0b'
  return '#ef4444'
}

function Bar({ pct }: { pct: number }) {
  return (
    <div style={{ width: 48, height: 4, background: 'var(--bg-elevated)', borderRadius: 2, overflow: 'hidden', flexShrink: 0 }}>
      <div style={{ width: `${Math.min(100, pct)}%`, height: '100%', background: barColor(pct), borderRadius: 2 }} />
    </div>
  )
}

export default function NativeStoragePanel() {
  const [devices, setDevices] = useState<StorageDevice[]>([])
  const [mounts,  setMounts]  = useState<StorageMount[]>([])
  const [raids,   setRaids]   = useState<RaidArray[]>([])
  const [loading, setLoading] = useState(true)
  const [tab, setTab] = useState('devices')

  const load = useCallback(async () => {
    setLoading(true)
    if (tab === 'devices') {
      const r = await fetch('/api/storage/devices', { credentials: 'include' })
      if (r.ok) { const d = await r.json(); setDevices(d.devices ?? []) }
    } else if (tab === 'mounts') {
      const r = await fetch('/api/storage/mounts', { credentials: 'include' })
      if (r.ok) { const d = await r.json(); setMounts(d.mounts ?? []) }
    } else {
      try {
        const r = await fetch('/api/storage/raid', { credentials: 'include' })
        if (r.ok) { const d = await r.json(); setRaids(d.arrays ?? d.raids ?? []) }
        else setRaids([])
      } catch { setRaids([]) }
    }
    setLoading(false)
  }, [tab])

  useEffect(() => { load() }, [load])

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab}>
      {loading ? <LoadingState /> : tab === 'devices' ? (
        devices.length === 0 ? <EmptyState text="No devices" /> :
        devices.map(d => (
          <NativeRow key={d.name}>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{d.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>
                {fmt(d.size)}
                {d.type    ? ` · ${d.type}`    : ''}
                {d.fs_type ? ` · ${d.fs_type}` : ''}
                {d.model   ? ` · ${d.model}`   : ''}
              </div>
            </div>
          </NativeRow>
        ))
      ) : tab === 'mounts' ? (
        mounts.length === 0 ? <EmptyState text="No mounts" /> :
        mounts.map(m => (
          <NativeRow key={m.mountpoint}>
            <StatusDot color={pctDot(m.use_pct)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{m.mountpoint}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>
                {fmt(m.used)}/{fmt(m.total)} · {fmt(m.available)} free · {m.fstype}
              </div>
            </div>
            <Bar pct={m.use_pct} />
            <span style={{ fontSize: 9, color: barColor(m.use_pct), width: 28, textAlign: 'right' }}>{m.use_pct}%</span>
          </NativeRow>
        ))
      ) : (
        raids.length === 0 ? <EmptyState text="No RAID arrays" /> :
        raids.map(r => (
          <NativeRow key={r.name}>
            <StatusDot color={raidDot(r.state)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{r.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{r.level} · {r.state} · {r.members.length} members</div>
            </div>
          </NativeRow>
        ))
      )}
    </NativePanelShell>
  )
}
