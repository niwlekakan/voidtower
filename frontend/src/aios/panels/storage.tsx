import { useEffect, useState } from 'react'
import NativePanelShell, { NativeRow, StatusDot, EmptyState, LoadingState } from './NativePanelShell'

interface StorageDevice { name: string; size: number; type?: string; model?: string }
interface StorageMount { device: string; mountpoint: string; fstype: string; total: number; used: number; use_pct: number }

const TABS = [{ id: 'devices', label: 'Devices' }, { id: 'mounts', label: 'Mounts' }]

function fmt(b: number) {
  if (b > 1e12) return `${(b / 1e12).toFixed(1)}T`
  if (b > 1e9) return `${(b / 1e9).toFixed(1)}G`
  return `${(b / 1e6).toFixed(0)}M`
}

function pctColor(p: number) {
  return p > 90 ? '#ef4444' : p > 75 ? '#f59e0b' : '#22c55e'
}

export default function NativeStoragePanel() {
  const [devices, setDevices] = useState<StorageDevice[]>([])
  const [mounts, setMounts] = useState<StorageMount[]>([])
  const [loading, setLoading] = useState(true)
  const [tab, setTab] = useState('devices')

  async function load() {
    setLoading(true)
    if (tab === 'devices') {
      const r = await fetch('/api/storage/devices', { credentials: 'include' })
      if (r.ok) { const d = await r.json(); setDevices(d.devices ?? []) }
    } else {
      const r = await fetch('/api/storage/mounts', { credentials: 'include' })
      if (r.ok) { const d = await r.json(); setMounts(d.mounts ?? []) }
    }
    setLoading(false)
  }

  useEffect(() => { load() }, [tab])

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab}>
      {loading ? <LoadingState /> : tab === 'devices' ? (
        devices.length === 0 ? <EmptyState text="No devices" /> :
        devices.map(d => (
          <NativeRow key={d.name}>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{d.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{fmt(d.size)}{d.type ? ` · ${d.type}` : ''}{d.model ? ` · ${d.model}` : ''}</div>
            </div>
          </NativeRow>
        ))
      ) : (
        mounts.length === 0 ? <EmptyState text="No mounts" /> :
        mounts.map(m => (
          <NativeRow key={m.mountpoint}>
            <StatusDot color={pctColor(m.use_pct)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{m.mountpoint}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{fmt(m.used)}/{fmt(m.total)} · {m.use_pct}% · {m.fstype}</div>
            </div>
          </NativeRow>
        ))
      )}
    </NativePanelShell>
  )
}
