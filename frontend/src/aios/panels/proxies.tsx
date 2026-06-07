import { useEffect, useState } from 'react'
import { ToggleLeft, ToggleRight, Trash2, Pencil, Plus } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Proxy { id: string; name?: string; domain: string; upstream: string; ssl: boolean; enabled: boolean; allow_embed?: boolean }

type Modal = { type: 'new' } | { type: 'edit'; proxy: Proxy }
const empty = { domain: '', upstream: '', ssl: false, allow_embed: false }

export default function NativeProxiesPanel() {
  const [proxies, setProxies] = useState<Proxy[]>([])
  const [loading, setLoading] = useState(true)
  const [modal, setModal] = useState<Modal | null>(null)
  const [form, setForm] = useState(empty)

  async function load() {
    const r = await fetch('/api/proxy', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setProxies(d.proxies ?? d ?? []) }
    setLoading(false)
  }
  async function toggle(id: string) {
    await fetch(`/api/proxy/${id}/toggle`, { method: 'POST', credentials: 'include' })
    load()
  }
  async function remove(id: string) {
    await fetch(`/api/proxy/${id}`, { method: 'DELETE', credentials: 'include' })
    load()
  }
  async function submit() {
    if (modal?.type === 'edit') {
      await fetch(`/api/proxy/${modal.proxy.id}`, { method: 'PUT', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(form) })
    } else {
      await fetch('/api/proxy', { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(form) })
    }
    setModal(null); load()
  }

  useEffect(() => { load() }, [])
  useEffect(() => {
    if (!modal) return
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape') setModal(null) }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [modal])

  return (
    <NativePanelShell actions={
      <IconBtn title="New proxy" onClick={() => { setForm(empty); setModal({ type: 'new' }) }}><Plus size={12} /></IconBtn>
    }>
      {loading ? <LoadingState /> : proxies.length === 0 ? <EmptyState text="No proxies" /> :
        proxies.map(p => (
          <NativeRow key={p.id}>
            <StatusDot color={p.enabled ? '#22c55e' : '#94a3b8'} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
                <span style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{p.name ?? p.domain}</span>
                {p.ssl && <span style={{ fontSize: 9, color: 'var(--accent-primary)', flexShrink: 0 }}>SSL</span>}
              </div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)', fontFamily: 'monospace', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{p.upstream}</div>
            </div>
            <IconBtn title="Edit" onClick={() => { setForm({ domain: p.domain, upstream: p.upstream, ssl: p.ssl, allow_embed: p.allow_embed ?? false }); setModal({ type: 'edit', proxy: p }) }}><Pencil size={11} /></IconBtn>
            <IconBtn title={p.enabled ? 'Disable' : 'Enable'} onClick={() => toggle(p.id)}>{p.enabled ? <ToggleRight size={13} /> : <ToggleLeft size={13} />}</IconBtn>
            <IconBtn title="Delete" onClick={() => remove(p.id)} danger><Trash2 size={11} /></IconBtn>
          </NativeRow>
        ))
      }
      {modal && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          onClick={() => setModal(null)}>
          <div style={{ background: 'var(--bg-panel)', borderRadius: 8, padding: 16, width: 300, border: '1px solid var(--border-subtle)' }}
            onClick={e => e.stopPropagation()}>
            <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 10 }}>{modal.type === 'new' ? 'New Proxy' : 'Edit Proxy'}</div>
            {(['domain', 'upstream'] as const).map(f => (
              <div key={f} style={{ marginBottom: 8 }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3, textTransform: 'capitalize' }}>{f}</div>
                <input value={form[f]} onChange={e => setForm(p => ({ ...p, [f]: e.target.value }))}
                  style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
              </div>
            ))}
            {(['ssl', 'allow_embed'] as const).map(f => (
              <label key={f} style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 8, cursor: 'pointer' }}>
                <input type="checkbox" checked={form[f]} onChange={e => setForm(p => ({ ...p, [f]: e.target.checked }))} />
                <span style={{ fontSize: 11, color: 'var(--text-primary)' }}>{f === 'ssl' ? 'SSL' : 'Allow Embed'}</span>
              </label>
            ))}
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 6, marginTop: 12 }}>
              <button onClick={() => setModal(null)} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-muted)', cursor: 'pointer' }}>Cancel</button>
              <button onClick={submit} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer' }}>Save</button>
            </div>
          </div>
        </div>
      )}
    </NativePanelShell>
  )
}
