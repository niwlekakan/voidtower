import { useEffect, useState } from 'react'
import NativePanelShell, { NativeRow, StatusDot, EmptyState, LoadingState } from './NativePanelShell'

interface NetIface { name: string; rx_bytes: number; tx_bytes: number; speed?: number }
interface Neighbor { ip: string; mac?: string; iface?: string; hostname?: string }

const TABS = [{ id: 'interfaces', label: 'Interfaces' }, { id: 'neighbors', label: 'Neighbors' }]

function fmt(b: number) {
  if (b > 1e9) return `${(b / 1e9).toFixed(1)}G`
  if (b > 1e6) return `${(b / 1e6).toFixed(1)}M`
  if (b > 1e3) return `${(b / 1e3).toFixed(1)}K`
  return `${b}B`
}

export default function NativeNetworkPanel() {
  const [ifaces, setIfaces] = useState<NetIface[]>([])
  const [neighbors, setNeighbors] = useState<Neighbor[]>([])
  const [loading, setLoading] = useState(true)
  const [tab, setTab] = useState('interfaces')

  async function load() {
    if (tab === 'interfaces') {
      const r = await fetch('/api/metrics/current', { credentials: 'include' })
      if (r.ok) { const d = await r.json(); setIfaces(d.network ?? []) }
    } else {
      const r = await fetch('/api/network/neighbors', { credentials: 'include' })
      if (r.ok) { const d = await r.json(); setNeighbors(d.neighbors ?? []) }
    }
    setLoading(false)
  }

  useEffect(() => { setLoading(true); load() }, [tab])

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab}>
      {loading ? <LoadingState /> : tab === 'interfaces' ? (
        ifaces.length === 0 ? <EmptyState text="No interfaces" /> :
        ifaces.map(i => (
          <NativeRow key={i.name}>
            <StatusDot color="#22c55e" />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{i.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>↑{fmt(i.tx_bytes)} ↓{fmt(i.rx_bytes)}</div>
            </div>
          </NativeRow>
        ))
      ) : (
        neighbors.length === 0 ? <EmptyState text="No neighbors" /> :
        neighbors.map(n => (
          <NativeRow key={n.ip}>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{n.hostname ?? n.ip}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{n.ip}{n.mac ? ` · ${n.mac}` : ''}{n.iface ? ` · ${n.iface}` : ''}</div>
            </div>
          </NativeRow>
        ))
      )}
    </NativePanelShell>
  )
}
