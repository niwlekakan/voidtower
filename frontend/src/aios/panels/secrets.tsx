import { useEffect, useState } from 'react'
import { Eye, EyeOff, RefreshCw, Trash2, Plus } from 'lucide-react'
import NativePanelShell, { NativeRow, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Secret { id: string; name: string; scope?: string; version?: number; created_at?: string; last_used_at?: string }

type Modal = { type: 'new' } | { type: 'rotate'; id: string; name: string }

function rel(ts?: string) {
  if (!ts) return ''
  const d = Date.now() - new Date(ts).getTime()
  if (d < 3600000) return `${Math.floor(d / 60000)}m ago`
  if (d < 86400000) return `${Math.floor(d / 3600000)}h ago`
  return `${Math.floor(d / 86400000)}d ago`
}

export default function NativeSecretsPanel() {
  const [secrets, setSecrets] = useState<Secret[]>([])
  const [revealed, setRevealed] = useState<Record<string, string>>({})
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')
  const [modal, setModal] = useState<Modal | null>(null)
  const [newForm, setNewForm] = useState({ name: '', value: '', scope: '' })
  const [rotateVal, setRotateVal] = useState('')

  async function load() {
    const r = await fetch('/api/secrets', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setSecrets(d.secrets ?? []) }
    setLoading(false)
  }
  async function reveal(id: string) {
    if (revealed[id]) { setRevealed(prev => { const n = { ...prev }; delete n[id]; return n }); return }
    const r = await fetch(`/api/secrets/${id}/reveal`, { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setRevealed(prev => ({ ...prev, [id]: d.value ?? '•••' })) }
  }
  async function remove(id: string) {
    await fetch(`/api/secrets/${id}`, { method: 'DELETE', credentials: 'include' })
    load()
  }
  async function submitNew() {
    const body: Record<string, string> = { name: newForm.name, value: newForm.value }
    if (newForm.scope) body.scope = newForm.scope
    await fetch('/api/secrets', { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
    setModal(null); load()
  }
  async function submitRotate() {
    if (modal?.type !== 'rotate') return
    await fetch(`/api/secrets/${modal.id}/rotate`, { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ value: rotateVal }) })
    setModal(null); load()
  }

  useEffect(() => { load() }, [])
  useEffect(() => {
    if (!modal) return
    const h = (e: KeyboardEvent) => { if (e.key === 'Escape') setModal(null) }
    window.addEventListener('keydown', h)
    return () => window.removeEventListener('keydown', h)
  }, [modal])

  const filtered = secrets.filter(s => s.name.toLowerCase().includes(search.toLowerCase()))

  return (
    <NativePanelShell search={search} onSearch={setSearch} searchPlaceholder="Filter secrets…" actions={
      <IconBtn title="New secret" onClick={() => { setNewForm({ name: '', value: '', scope: '' }); setModal({ type: 'new' }) }}><Plus size={12} /></IconBtn>
    }>
      {loading ? <LoadingState /> : filtered.length === 0 ? <EmptyState text="No secrets" /> :
        filtered.map(s => (
          <NativeRow key={s.id}>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{s.name}</div>
              {revealed[s.id]
                ? <div style={{ fontSize: 9, color: 'var(--text-muted)', fontFamily: 'monospace', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{revealed[s.id]}</div>
                : <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>
                    {[s.scope, s.version != null ? `v${s.version}` : null, rel(s.created_at)].filter(Boolean).join(' · ')}
                  </div>
              }
            </div>
            <IconBtn title={revealed[s.id] ? 'Hide' : 'Reveal'} onClick={() => reveal(s.id)}>
              {revealed[s.id] ? <EyeOff size={11} /> : <Eye size={11} />}
            </IconBtn>
            <IconBtn title="Rotate" onClick={() => { setRotateVal(''); setModal({ type: 'rotate', id: s.id, name: s.name }) }}><RefreshCw size={11} /></IconBtn>
            <IconBtn title="Delete" onClick={() => remove(s.id)} danger><Trash2 size={11} /></IconBtn>
          </NativeRow>
        ))
      }
      {modal?.type === 'new' && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          onClick={() => setModal(null)}>
          <div style={{ background: 'var(--bg-panel)', borderRadius: 8, padding: 16, width: 300, border: '1px solid var(--border-subtle)' }}
            onClick={e => e.stopPropagation()}>
            <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 10 }}>New Secret</div>
            {([{ label: 'Name', key: 'name' }, { label: 'Scope (optional)', key: 'scope' }] as { label: string; key: 'name' | 'scope' }[]).map(({ label, key }) => (
              <div key={key} style={{ marginBottom: 8 }}>
                <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>{label}</div>
                <input value={newForm[key]} onChange={e => setNewForm(p => ({ ...p, [key]: e.target.value }))}
                  style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
              </div>
            ))}
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Value</div>
              <textarea value={newForm.value} onChange={e => setNewForm(p => ({ ...p, value: e.target.value }))} rows={3}
                style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none', resize: 'vertical', fontFamily: 'monospace' }} />
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 6, marginTop: 12 }}>
              <button onClick={() => setModal(null)} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-muted)', cursor: 'pointer' }}>Cancel</button>
              <button onClick={submitNew} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer' }}>Save</button>
            </div>
          </div>
        </div>
      )}
      {modal?.type === 'rotate' && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          onClick={() => setModal(null)}>
          <div style={{ background: 'var(--bg-panel)', borderRadius: 8, padding: 16, width: 300, border: '1px solid var(--border-subtle)' }}
            onClick={e => e.stopPropagation()}>
            <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 4 }}>Rotate Secret</div>
            <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 10 }}>{modal.name}</div>
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>New value</div>
              <textarea value={rotateVal} onChange={e => setRotateVal(e.target.value)} rows={3}
                style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none', resize: 'vertical', fontFamily: 'monospace' }} />
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 6, marginTop: 12 }}>
              <button onClick={() => setModal(null)} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: '1px solid var(--border-subtle)', background: 'none', color: 'var(--text-muted)', cursor: 'pointer' }}>Cancel</button>
              <button onClick={submitRotate} style={{ fontSize: 11, padding: '4px 10px', borderRadius: 4, border: 'none', background: 'var(--accent-primary)', color: '#fff', cursor: 'pointer' }}>Rotate</button>
            </div>
          </div>
        </div>
      )}
    </NativePanelShell>
  )
}
