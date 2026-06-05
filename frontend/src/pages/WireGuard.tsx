import { useState, useEffect, useCallback } from 'react'
import { Shield, Plus, Trash2, Copy, Check, ChevronDown, ChevronUp, Wifi, WifiOff } from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import { notify } from '@/store/notifications'
import type { WgPeer, WgInterface } from '@/api/types'
import Button from '@/components/ui/Button'

function fmtBytes(b: number) {
  if (b < 1024) return `${b} B`
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)} KB`
  if (b < 1024 * 1024 * 1024) return `${(b / 1024 / 1024).toFixed(1)} MB`
  return `${(b / 1024 / 1024 / 1024).toFixed(2)} GB`
}

function fmtHandshake(ts: number | null) {
  if (!ts) return 'Never'
  const ago = Math.floor(Date.now() / 1000) - ts
  if (ago < 60) return `${ago}s ago`
  if (ago < 3600) return `${Math.floor(ago / 60)}m ago`
  if (ago < 86400) return `${Math.floor(ago / 3600)}h ago`
  return `${Math.floor(ago / 86400)}d ago`
}

function ConfigModal({ config, onClose }: { config: string; onClose: () => void }) {
  const [copied, setCopied] = useState(false)

  const copy = () => {
    navigator.clipboard.writeText(config).then(() => {
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    })
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4"
         style={{ background: 'rgba(0,0,0,0.7)' }}
         onClick={(e) => e.target === e.currentTarget && onClose()}>
      <div className="rounded-xl shadow-2xl w-full max-w-lg space-y-4 p-5"
           style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-default)' }}>
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
            Client Configuration
          </h2>
          <button onClick={copy}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded text-xs transition-colors hover:opacity-80"
            style={{ background: 'var(--accent-primary-subtle)', color: 'var(--accent-primary)', border: '1px solid var(--accent-primary)' }}>
            {copied ? <Check size={12} /> : <Copy size={12} />}
            {copied ? 'Copied!' : 'Copy'}
          </button>
        </div>

        <pre className="text-xs font-mono p-4 rounded overflow-x-auto whitespace-pre"
             style={{ background: 'var(--bg-elevated)', color: 'var(--terminal-green)', border: '1px solid var(--border-subtle)', maxHeight: 340 }}>
          {config}
        </pre>

        <div className="p-3 rounded text-xs space-y-1"
             style={{ background: 'var(--accent-warning-subtle)', color: 'var(--accent-warning)', border: '1px solid var(--accent-warning)' }}>
          <p className="font-medium">Save this config now — the private key cannot be recovered.</p>
          <p>Mobile: paste into the WireGuard app → + → Create from clipboard or QR scan of this text.</p>
        </div>

        <div className="flex justify-end">
          <Button size="sm" variant="ghost" onClick={onClose}>Close</Button>
        </div>
      </div>
    </div>
  )
}

export default function WireGuardPage() {
  const [data, setData] = useState<{ available: boolean; error: string | null; interfaces: WgInterface[]; peers: WgPeer[] } | null>(null)
  const [loading, setLoading] = useState(true)
  const [showAdd, setShowAdd] = useState(false)

  // Add form
  const [peerName, setPeerName] = useState('')
  const [iface, setIface] = useState('wg0')
  const [endpoint, setEndpoint] = useState('')
  const [adding, setAdding] = useState(false)

  // Config modal
  const [pendingConfig, setPendingConfig] = useState<string | null>(null)

  // Expanded rows for public key
  const [expanded, setExpanded] = useState<Set<string>>(new Set())

  const refresh = useCallback(() => {
    setLoading(true)
    api.wireguard.list()
      .then(setData)
      .catch(() => notify.error('Failed to load WireGuard status'))
      .finally(() => setLoading(false))
  }, [])

  useEffect(() => { refresh() }, [refresh])

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!peerName.trim()) { notify.error('Enter a peer name'); return }
    setAdding(true)
    try {
      const res = await api.wireguard.addPeer(peerName.trim(), iface, endpoint.trim() || undefined)
      notify.success(`Peer "${peerName}" added — IP ${res.allocated_ip}`)
      if (res.warnings.length) res.warnings.forEach(w => notify.error(`Warning: ${w}`))
      setPendingConfig(res.client_config)
      setPeerName(''); setEndpoint(''); setShowAdd(false)
      refresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to add peer')
    } finally {
      setAdding(false)
    }
  }

  const handleDelete = async (p: WgPeer) => {
    if (!confirm(`Remove peer "${p.name}" (${p.allocated_ip})? This cannot be undone.`)) return
    try {
      const res = await api.wireguard.deletePeer(p.id)
      if (res.warnings.length) res.warnings.forEach(w => notify.error(`Warning: ${w}`))
      notify.success(`Peer "${p.name}" removed`)
      refresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to remove peer')
    }
  }

  const toggleExpand = (id: string) =>
    setExpanded(prev => { const s = new Set(prev); if (s.has(id)) s.delete(id); else s.add(id); return s })

  const peers = data?.peers ?? []
  const ifaces = data?.interfaces ?? []

  return (
    <div className="space-y-5 max-w-4xl">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>WireGuard VPN</h1>
        <div className="flex items-center gap-3">
          <span className="text-xs flex items-center gap-1.5"
                style={{ color: data?.available ? 'var(--accent-success)' : 'var(--accent-danger)' }}>
            <span className="w-1.5 h-1.5 rounded-full" style={{ background: 'currentColor' }} />
            {data?.available ? 'wg available' : 'wg not found'}
          </span>
          <Button size="sm" variant="primary" onClick={() => setShowAdd(v => !v)}>
            <Plus size={13} className="mr-1" /> Add peer
          </Button>
        </div>
      </div>

      {data?.error && (
        <div className="p-3 rounded text-xs"
             style={{ background: 'var(--accent-warning-subtle)', color: 'var(--accent-warning)', border: '1px solid var(--accent-warning)' }}>
          {data.error}
        </div>
      )}

      {/* Interfaces */}
      {ifaces.length > 0 && (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {ifaces.map(i => (
            <div key={i.name} className="card space-y-2">
              <div className="flex items-center gap-2">
                <Shield size={14} style={{ color: 'var(--accent-primary)' }} />
                <span className="text-sm font-mono font-medium" style={{ color: 'var(--text-primary)' }}>{i.name}</span>
              </div>
              <div className="text-xs space-y-1" style={{ color: 'var(--text-muted)' }}>
                <div>Port: <span className="font-mono" style={{ color: 'var(--text-secondary)' }}>{i.listen_port}</span></div>
                <div className="font-mono truncate text-xs" title={i.public_key}
                     style={{ color: 'var(--text-secondary)' }}>
                  {i.public_key ? `${i.public_key.slice(0, 20)}…` : '(no public key)'}
                </div>
                <div>{peers.filter(p => p.interface === i.name).length} peer(s) configured</div>
              </div>
            </div>
          ))}
        </div>
      )}

      {!data?.available && !loading && (
        <div className="p-4 rounded-lg text-xs space-y-1"
             style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-muted)' }}>
          <p className="font-medium" style={{ color: 'var(--text-secondary)' }}>WireGuard not detected</p>
          <p>Install: <code className="font-mono">sudo apt install wireguard</code> (Debian/Ubuntu) or <code className="font-mono">sudo pacman -S wireguard-tools</code> (Arch)</p>
          <p>Then create an interface: <code className="font-mono">sudo wg genkey | sudo tee /etc/wireguard/wg0.key | wg pubkey | sudo tee /etc/wireguard/wg0.pub</code></p>
          <p>You can still add peers here — they will be synced once WireGuard is installed.</p>
        </div>
      )}

      {/* Add form */}
      {showAdd && (
        <form onSubmit={handleAdd} className="card space-y-4">
          <h2 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>New peer</h2>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Peer name</label>
              <input value={peerName} onChange={e => setPeerName(e.target.value)}
                placeholder="e.g. laptop, phone"
                className="w-full px-3 py-2 rounded text-sm outline-none"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
                required />
            </div>
            <div>
              <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Interface</label>
              <input value={iface} onChange={e => setIface(e.target.value)}
                placeholder="wg0"
                className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }} />
            </div>
          </div>
          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>
              Server endpoint <span style={{ color: 'var(--text-disabled)' }}>(optional — for the client config)</span>
            </label>
            <input value={endpoint} onChange={e => setEndpoint(e.target.value)}
              placeholder="your.server.ip or hostname"
              className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
              style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }} />
          </div>
          <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
            A Curve25519 keypair is generated server-side. The client config (with private key) is shown once — save it immediately.
          </p>
          <div className="flex gap-2">
            <Button size="sm" variant="primary" type="submit" loading={adding}>Generate &amp; add</Button>
            <Button size="sm" variant="ghost" type="button" onClick={() => setShowAdd(false)}>Cancel</Button>
          </div>
        </form>
      )}

      {/* Peers table */}
      <div className="panel overflow-hidden">
        {loading ? (
          <p className="px-4 py-6 text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</p>
        ) : peers.length === 0 ? (
          <div className="flex flex-col items-center gap-3 py-12">
            <Shield size={32} style={{ color: 'var(--text-muted)', opacity: 0.4 }} />
            <p className="text-sm" style={{ color: 'var(--text-muted)' }}>No peers configured.</p>
            <Button size="sm" variant="ghost" onClick={() => setShowAdd(true)}>
              <Plus size={12} className="mr-1" /> Add first peer
            </Button>
          </div>
        ) : (
          <table className="w-full text-xs">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                {['', 'Name', 'IP', 'Endpoint', 'Handshake', 'Traffic', 'Interface', ''].map((h, i) => (
                  <th key={i} className="px-4 py-2.5 text-left uppercase tracking-wider"
                      style={{ color: 'var(--text-muted)' }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {peers.map(p => (
                <>
                  <tr key={p.id} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                    <td className="px-3 py-3">
                      <span title={p.connected ? 'Connected' : 'Offline'}>
                        {p.connected
                          ? <Wifi size={13} style={{ color: 'var(--accent-success)' }} />
                          : <WifiOff size={13} style={{ color: 'var(--text-disabled)' }} />}
                      </span>
                    </td>
                    <td className="px-4 py-3 font-medium" style={{ color: 'var(--text-primary)' }}>{p.name}</td>
                    <td className="px-4 py-3 font-mono" style={{ color: 'var(--accent-secondary)' }}>{p.allocated_ip}</td>
                    <td className="px-4 py-3 font-mono" style={{ color: 'var(--text-muted)' }}>
                      {p.endpoint ?? '—'}
                    </td>
                    <td className="px-4 py-3" style={{ color: p.connected ? 'var(--accent-success)' : 'var(--text-muted)' }}>
                      {fmtHandshake(p.latest_handshake)}
                    </td>
                    <td className="px-4 py-3" style={{ color: 'var(--text-muted)' }}>
                      {p.rx_bytes > 0 || p.tx_bytes > 0
                        ? <span>↓{fmtBytes(p.rx_bytes)} ↑{fmtBytes(p.tx_bytes)}</span>
                        : '—'}
                    </td>
                    <td className="px-4 py-3 font-mono" style={{ color: 'var(--text-muted)' }}>{p.interface}</td>
                    <td className="px-4 py-3">
                      <div className="flex items-center gap-1">
                        <button onClick={() => toggleExpand(p.id)} title="Show public key"
                          className="p-1 rounded hover:opacity-80"
                          style={{ color: 'var(--text-muted)' }}>
                          {expanded.has(p.id) ? <ChevronUp size={13} /> : <ChevronDown size={13} />}
                        </button>
                        <button onClick={() => handleDelete(p)} title="Remove peer"
                          className="p-1 rounded hover:opacity-80"
                          style={{ color: 'var(--accent-danger)' }}>
                          <Trash2 size={13} />
                        </button>
                      </div>
                    </td>
                  </tr>
                  {expanded.has(p.id) && (
                    <tr key={`${p.id}-exp`} style={{ borderBottom: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)' }}>
                      <td colSpan={8} className="px-4 py-2">
                        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>Public key: </span>
                        <code className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>{p.public_key}</code>
                      </td>
                    </tr>
                  )}
                </>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Config modal */}
      {pendingConfig && (
        <ConfigModal config={pendingConfig} onClose={() => setPendingConfig(null)} />
      )}
    </div>
  )
}
