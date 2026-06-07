import { useEffect, useState } from 'react'
import { Play, Pencil, Trash2, Plus } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Automation { id: string; name: string; command?: string; enabled: boolean; last_run?: string; schedule?: string; last_exit_code?: number }

type Modal = { type: 'new' } | { type: 'edit'; item: Automation }
const empty = { name: '', command: '', schedule: '', enabled: true }

export default function NativeAutomationPanel() {
  const [items, setItems] = useState<Automation[]>([])
  const [loading, setLoading] = useState(true)
  const [modal, setModal] = useState<Modal | null>(null)
  const [form, setForm] = useState(empty)

  async function load() {
    const r = await fetch('/api/automation', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setItems(d.automations ?? d ?? []) }
    setLoading(false)
  }
  async function run(id: string) {
    await fetch(`/api/automation/${id}/run`, { method: 'POST', credentials: 'include' })
    load()
  }
  async function remove(id: string) {
    await fetch(`/api/automation/${id}`, { method: 'DELETE', credentials: 'include' })
    load()
  }
  async function submit() {
    const body = { name: form.name, command: form.command, schedule: form.schedule, enabled: form.enabled }
    if (modal?.type === 'edit') {
      await fetch(`/api/automation/${modal.item.id}`, { method: 'PUT', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
    } else {
      await fetch('/api/automation', { method: 'POST', credentials: 'include', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
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

  function exitColor(code?: number) {
    if (code == null) return '#94a3b8'
    return code === 0 ? '#22c55e' : '#ef4444'
  }

  return (
    <NativePanelShell actions={
      <IconBtn title="New automation" onClick={() => { setForm(empty); setModal({ type: 'new' }) }}><Plus size={12} /></IconBtn>
    }>
      {loading ? <LoadingState /> : items.length === 0 ? <EmptyState text="No automations" /> :
        items.map(a => (
          <NativeRow key={a.id}>
            <StatusDot color={exitColor(a.last_exit_code)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{a.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)', fontFamily: 'monospace', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {a.command ? a.command.slice(0, 40) + (a.command.length > 40 ? '…' : '') : (a.schedule ?? 'manual')}
              </div>
            </div>
            <IconBtn title="Run now" onClick={() => run(a.id)}><Play size={11} /></IconBtn>
            <IconBtn title="Edit" onClick={() => { setForm({ name: a.name, command: a.command ?? '', schedule: a.schedule ?? '', enabled: a.enabled }); setModal({ type: 'edit', item: a }) }}><Pencil size={11} /></IconBtn>
            <IconBtn title="Delete" onClick={() => remove(a.id)} danger><Trash2 size={11} /></IconBtn>
          </NativeRow>
        ))
      }
      {modal && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 50, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
          onClick={() => setModal(null)}>
          <div style={{ background: 'var(--bg-panel)', borderRadius: 8, padding: 16, width: 300, border: '1px solid var(--border-subtle)' }}
            onClick={e => e.stopPropagation()}>
            <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-primary)', marginBottom: 10 }}>{modal.type === 'new' ? 'New Automation' : 'Edit Automation'}</div>
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Name</div>
              <input value={form.name} onChange={e => setForm(p => ({ ...p, name: e.target.value }))}
                style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
            </div>
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Command</div>
              <textarea value={form.command} onChange={e => setForm(p => ({ ...p, command: e.target.value })) } rows={3}
                style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none', resize: 'vertical', fontFamily: 'monospace' }} />
            </div>
            <div style={{ marginBottom: 8 }}>
              <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 3 }}>Schedule (cron)</div>
              <input value={form.schedule} onChange={e => setForm(p => ({ ...p, schedule: e.target.value }))} placeholder="e.g. 0 * * * *"
                style={{ width: '100%', boxSizing: 'border-box', background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', borderRadius: 4, padding: '4px 6px', fontSize: 11, color: 'var(--text-primary)', outline: 'none' }} />
            </div>
            <label style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 8, cursor: 'pointer' }}>
              <input type="checkbox" checked={form.enabled} onChange={e => setForm(p => ({ ...p, enabled: e.target.checked }))} />
              <span style={{ fontSize: 11, color: 'var(--text-primary)' }}>Enabled</span>
            </label>
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
