import { useState, useCallback } from 'react'
import { Wifi, RefreshCw, Monitor } from 'lucide-react'
import { useMetricsStore } from '@/store/metrics'

function fmt(bytes: number) {
  if (!bytes) return '0 B'
  const k = 1024, s = ['B','KB','MB','GB','TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${s[i]}`
}

interface Neighbor {
  ip: string
  mac: string
  iface: string
  state: string
  hostname: string | null
}

export default function NetworkPage() {
  const snapshot = useMetricsStore((s) => s.snapshot)

  const [neighbors, setNeighbors] = useState<Neighbor[] | null>(null)
  const [scanning, setScanning] = useState(false)
  const [scanError, setScanError] = useState<string | null>(null)

  const scan = useCallback(async () => {
    setScanning(true)
    setScanError(null)
    try {
      const res = await fetch('/api/network/neighbors', { credentials: 'include' })
      if (!res.ok) throw new Error(`${res.status}`)
      const data = await res.json()
      setNeighbors(data.neighbors)
    } catch (e) {
      setScanError(e instanceof Error ? e.message : 'Scan failed')
    } finally {
      setScanning(false)
    }
  }, [])

  const ifaces = snapshot?.networks.filter(n => n.rx_bytes > 0 || n.tx_bytes > 0) ?? []

  return (
    <div className="space-y-5">
      <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Network</h1>

      {/* Interfaces */}
      <div className="panel overflow-hidden">
        <div className="px-4 py-2.5 text-xs uppercase tracking-wider font-medium"
             style={{ color: 'var(--text-muted)', borderBottom: '1px solid var(--border-subtle)' }}>
          Interfaces
        </div>
        {!snapshot ? (
          <p className="px-4 py-6 text-xs" style={{ color: 'var(--text-muted)' }}>Waiting for metrics…</p>
        ) : (
          <div className="overflow-x-auto">
          <table className="w-full text-xs">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                {['Interface', 'RX Total', 'TX Total', 'RX/s', 'TX/s'].map(h => (
                  <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider"
                      style={{ color: 'var(--text-muted)' }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {ifaces.map((n) => (
                <tr key={n.name} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  <td className="px-4 py-3 font-mono font-medium flex items-center gap-2"
                      style={{ color: 'var(--text-primary)' }}>
                    <Wifi size={12} style={{ color: 'var(--accent-primary)', flexShrink: 0 }} />
                    {n.name}
                  </td>
                  <td className="px-4 py-3 font-mono" style={{ color: 'var(--text-secondary)' }}>{fmt(n.rx_bytes)}</td>
                  <td className="px-4 py-3 font-mono" style={{ color: 'var(--text-secondary)' }}>{fmt(n.tx_bytes)}</td>
                  <td className="px-4 py-3 font-mono" style={{ color: 'var(--accent-secondary)' }}>↓ {fmt(n.rx_bytes_per_sec)}/s</td>
                  <td className="px-4 py-3 font-mono" style={{ color: 'var(--accent-primary)' }}>↑ {fmt(n.tx_bytes_per_sec)}/s</td>
                </tr>
              ))}
              {ifaces.length === 0 && (
                <tr><td colSpan={5} className="px-4 py-8 text-center"
                        style={{ color: 'var(--text-muted)' }}>No active interfaces detected.</td></tr>
              )}
            </tbody>
          </table>
          </div>
        )}
      </div>

      {/* LAN Neighbors */}
      <div className="panel overflow-hidden">
        <div className="flex items-center justify-between px-4 py-2.5"
             style={{ borderBottom: '1px solid var(--border-subtle)' }}>
          <div className="flex items-center gap-2">
            <Monitor size={13} style={{ color: 'var(--accent-secondary)' }} />
            <span className="text-xs uppercase tracking-wider font-medium"
                  style={{ color: 'var(--text-muted)' }}>
              LAN Neighbors
              {neighbors && (
                <span className="ml-2 normal-case font-normal"
                      style={{ color: 'var(--text-disabled)' }}>
                  {neighbors.length} device{neighbors.length !== 1 ? 's' : ''}
                </span>
              )}
            </span>
          </div>
          <button
            onClick={scan}
            disabled={scanning}
            className="flex items-center gap-1.5 px-3 py-1 rounded text-xs transition-colors hover:opacity-80 disabled:opacity-50"
            style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
          >
            <RefreshCw size={11} className={scanning ? 'animate-spin' : ''} />
            {neighbors === null ? 'Scan' : scanning ? 'Scanning…' : 'Refresh'}
          </button>
        </div>

        {scanError && (
          <div className="px-4 py-3 text-xs"
               style={{ color: 'var(--accent-danger)', background: 'var(--accent-danger-subtle)' }}>
            {scanError}
          </div>
        )}

        {neighbors === null && !scanning && !scanError && (
          <div className="flex flex-col items-center gap-3 py-10">
            <Monitor size={28} style={{ color: 'var(--text-muted)', opacity: 0.35 }} />
            <p className="text-sm" style={{ color: 'var(--text-muted)' }}>
              Reads the ARP cache — shows devices your host has recently communicated with.
            </p>
            <button
              onClick={scan}
              className="px-4 py-1.5 rounded text-xs transition-colors hover:opacity-80"
              style={{ background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)', border: '1px solid var(--accent-primary)' }}
            >
              Scan now
            </button>
          </div>
        )}

        {scanning && neighbors === null && (
          <p className="px-4 py-8 text-xs text-center" style={{ color: 'var(--text-muted)' }}>
            Reading ARP table…
          </p>
        )}

        {neighbors !== null && neighbors.length === 0 && (
          <p className="px-4 py-8 text-xs text-center" style={{ color: 'var(--text-muted)' }}>
            No neighbors found. Try pinging devices on your LAN first to populate the ARP cache.
          </p>
        )}

        {neighbors !== null && neighbors.length > 0 && (
          <div className="overflow-x-auto">
          <table className="w-full text-xs">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                {['IP Address', 'Hostname', 'MAC Address', 'Interface', 'State'].map(h => (
                  <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider"
                      style={{ color: 'var(--text-muted)' }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {neighbors.map((n, i) => (
                <tr key={i} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                  <td className="px-4 py-2.5 font-mono font-medium"
                      style={{ color: 'var(--accent-secondary)' }}>{n.ip}</td>
                  <td className="px-4 py-2.5" style={{ color: 'var(--text-primary)' }}>
                    {n.hostname ?? <span style={{ color: 'var(--text-disabled)' }}>—</span>}
                  </td>
                  <td className="px-4 py-2.5 font-mono" style={{ color: 'var(--text-muted)' }}>
                    {n.mac}
                  </td>
                  <td className="px-4 py-2.5 font-mono" style={{ color: 'var(--text-secondary)' }}>
                    {n.iface}
                  </td>
                  <td className="px-4 py-2.5">
                    <span className="px-1.5 py-0.5 rounded text-xs"
                          style={{
                            background: n.state === 'reachable' ? 'var(--accent-success-subtle)' : 'var(--bg-elevated)',
                            color: n.state === 'reachable' ? 'var(--accent-success)' : 'var(--text-muted)',
                          }}>
                      {n.state}
                    </span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          </div>
        )}
      </div>
    </div>
  )
}
