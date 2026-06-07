import { useEffect, useState } from 'react'
import { RefreshCw } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface NetIface { name: string; rx_bytes: number; tx_bytes: number; rx_bytes_per_sec?: number; tx_bytes_per_sec?: number }
interface Neighbor { ip: string; mac?: string; iface?: string; hostname?: string; state?: string }

const TABS = [{ id: 'interfaces', label: 'Interfaces' }, { id: 'neighbors', label: 'Neighbors' }]

function fmtRate(bps: number) {
  if (bps >= 1e6) return (bps / 1e6).toFixed(1) + ' MB/s'
  if (bps >= 1e3) return (bps / 1e3).toFixed(0) + ' KB/s'
  return bps.toFixed(0) + ' B/s'
}

function fmt(b: number) {
  if (b > 1e9) return `${(b / 1e9).toFixed(1)}G`
  if (b > 1e6) return `${(b / 1e6).toFixed(1)}M`
  if (b > 1e3) return `${(b / 1e3).toFixed(1)}K`
  return `${b}B`
}

function neighborDot(state?: string) {
  if (state === 'reachable') return '#22c55e'
  if (state === 'stale')     return '#f59e0b'
  return '#94a3b8'
}

export default function NativeNetworkPanel() {
  const [ifaces, setIfaces] = useState<NetIface[]>([])
  const [neighbors, setNeighbors] = useState<Neighbor[]>([])
  const [loading, setLoading] = useState(true)
  const [scanning, setScanning] = useState(false)
  const [tab, setTab] = useState('interfaces')

  async function loadIfaces() {
    setLoading(true)
    const r = await fetch('/api/metrics/current', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setIfaces(d.networks ?? []) }
    setLoading(false)
  }

  async function loadNeighbors() {
    setLoading(true)
    const r = await fetch('/api/network/neighbors', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setNeighbors(d.neighbors ?? []) }
    setLoading(false)
  }

  async function scan() {
    setScanning(true)
    const r = await fetch('/api/network/neighbors', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setNeighbors(d.neighbors ?? []) }
    setScanning(false)
  }

  useEffect(() => {
    if (tab === 'interfaces') loadIfaces()
    else loadNeighbors()
  }, [tab])

  const actions = tab === 'neighbors' ? (
    <IconBtn title={scanning ? 'Scanning…' : 'Scan neighbors'} onClick={scan}>
      <RefreshCw size={12} style={{ opacity: scanning ? 0.5 : 1, animation: scanning ? 'spin 1s linear infinite' : 'none' }} />
    </IconBtn>
  ) : undefined

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab} actions={actions}>
      {loading ? <LoadingState /> : tab === 'interfaces' ? (
        ifaces.length === 0 ? <EmptyState text="No interfaces" /> :
        ifaces.map(i => (
          <NativeRow key={i.name}>
            <StatusDot color="#22c55e" />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{i.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>
                {i.rx_bytes_per_sec !== undefined
                  ? `↓${fmtRate(i.rx_bytes_per_sec)} ↑${fmtRate(i.tx_bytes_per_sec ?? 0)}`
                  : `↑${fmt(i.tx_bytes)} ↓${fmt(i.rx_bytes)}`}
              </div>
            </div>
          </NativeRow>
        ))
      ) : (
        neighbors.length === 0 ? <EmptyState text="No neighbors — click scan" /> :
        neighbors.map(n => (
          <NativeRow key={n.ip}>
            <StatusDot color={neighborDot(n.state)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>
                {n.hostname ? `${n.hostname}` : n.ip}
                {n.hostname && <span style={{ fontSize: 9, color: 'var(--text-muted)', marginLeft: 4 }}>{n.ip}</span>}
              </div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>
                {n.mac ?? '—'}{n.iface ? ` · ${n.iface}` : ''}{n.state ? ` · ${n.state}` : ''}
              </div>
            </div>
          </NativeRow>
        ))
      )}
    </NativePanelShell>
  )
}
