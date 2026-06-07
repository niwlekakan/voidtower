import { useEffect, useState } from 'react'
import NativePanelShell, { NativeRow, StatusDot, EmptyState, LoadingState } from './NativePanelShell'

interface WgPeer { public_key: string; endpoint?: string; latest_handshake?: number; allowed_ips: string[] }
interface WgInterface { name: string; public_key: string; listen_port: number; peers: WgPeer[] }

export default function NativeWireGuardPanel() {
  const [ifaces, setIfaces] = useState<WgInterface[]>([])
  const [loading, setLoading] = useState(true)

  async function load() {
    const r = await fetch('/api/wireguard', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setIfaces(d.interfaces ?? []) }
    setLoading(false)
  }

  useEffect(() => { load() }, [])

  const peers = ifaces.flatMap(i => i.peers.map(p => ({ ...p, iface: i.name })))

  return (
    <NativePanelShell>
      {loading ? <LoadingState /> : ifaces.length === 0 ? <EmptyState text="No WireGuard interfaces" /> : <>
        {ifaces.map(i => (
          <NativeRow key={i.name} style={{ background: 'var(--bg-elevated)' }}>
            <StatusDot color="#22c55e" />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{i.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>port {i.listen_port} · {i.peers.length} peer{i.peers.length !== 1 ? 's' : ''}</div>
            </div>
          </NativeRow>
        ))}
        {peers.length === 0 ? <EmptyState text="No peers" /> : peers.map(p => (
          <NativeRow key={p.public_key} style={{ paddingLeft: 20 }}>
            <StatusDot color={p.latest_handshake && Date.now() / 1000 - p.latest_handshake < 180 ? '#22c55e' : '#94a3b8'} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{p.endpoint ?? p.public_key.slice(0, 16) + '…'}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{p.allowed_ips.join(', ')}</div>
            </div>
          </NativeRow>
        ))}
      </>}
    </NativePanelShell>
  )
}
