import { useEffect, useState } from 'react'
import { Trash2, Plus } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface WgPeer { public_key: string; name?: string; endpoint?: string; latest_handshake?: number; allowed_ips: string[]; rx_bytes?: number; tx_bytes?: number; iface: string }
interface WgInterface { name: string; public_key: string; listen_port: number; peers: WgPeer[] }

function fmt(bytes?: number) {
  if (!bytes) return '0 B'
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

const emptyForm = { name: '', iface: '', allowed_ips: '', endpoint: '' }

export default function NativeWireGuardPanel() {
  const [ifaces, setIfaces] = useState<WgInterface[]>([])
  const [loading, setLoading] = useState(true)
  const [modal, setModal] = useState(false)
  const [form, setForm] = useState(emptyForm)

  async function load() {
    const r = await fetch('/api/wireguard', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setIfaces(d.interfaces ?? []) }
    setLoading(false)
  }
  async function removePeer(public_key: string) {
    const encoded = encodeURIComponent(public_key)
    await fetch(`/api/wireguard/peers/${encoded}`, { method: 'DELETE', credentials: 'include' })
    load()
  }
  async function submit() {
    const body: Record<string, string> = { name: form.name, interface: form.iface, allowed_ips: form.allowed_ips }
    if (form.endpoint) body.endpoint = form.endpoint
    await fetch('/api/wireguard/peers', { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
    setModal(false); load()
  }

  useEffect(() => { load() }, [])
  useEffect(() => {
    if (!modal) return
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape') setModal(false) }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [modal])

  const peers = ifaces.flatMap(i => i.peers.map(p => ({ ...p, iface: i.name })))

  return (
    <NativePanelShell actions={
      <IconBtn title="Add peer" onClick={() => { setForm({ ...emptyForm, iface: ifaces[0]?.name ?? '' }); setModal(true) }}><Plus size={12} /></IconBtn>
    }>
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
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{p.name ?? p.endpoint ?? p.public_key.slice(0, 16) + '…'}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>↓{fmt(p.rx_bytes)} ↑{fmt(p.tx_bytes)} · {p.allowed_ips.join(', ')}</div>
            </div>
            <IconBtn title="Remove peer" onClick={() => removePeer(p.public_key)} danger><Trash2 size={11} /></IconBtn>
          </NativeRow>
        ))}
      </>}
      {modal && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          onClick={() => setModal(false)}>
          <div style={{ background: 'var(--bg-panel)', borderRadius: 8, padding: 16, width: 300, border: '1px solid var(--border-subtle)' }}
            onClick={e => e.stopPropagation()}>
            <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 10 }}>Add Peer</div>
            {([{ label: 'Name', key: 'name' }, { label: 'Allowed IPs', key: 'allowed_ips' }, { label: 'Endpoint (optional)', key: 'endpoint' }] as { label: string; key: keyof typeof emptyForm }[]).map(({ label, key }) => (
              <div key={key} style={{ marginBottom: 8 }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>{label}</div>
                <input value={form[key]} onChange={e => setForm(p => ({ ...p, [key]: e.target.value }))}
                  style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
              </div>
            ))}
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Interface</div>
              <select value={form.iface} onChange={e => setForm(p => ({ ...p, iface: e.target.value }))}
                style={{ width: '100%', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }}>
                {ifaces.map(i => <option key={i.name} value={i.name}>{i.name}</option>)}
              </select>
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 6, marginTop: 12 }}>
              <button onClick={() => setModal(false)} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-muted)', cursor: 'pointer' }}>Cancel</button>
              <button onClick={submit} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer' }}>Add</button>
            </div>
          </div>
        </div>
      )}
    </NativePanelShell>
  )
}
