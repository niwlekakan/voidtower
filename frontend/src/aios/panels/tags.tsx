import { useEffect, useState } from 'react'
import { Trash2, Plus } from 'lucide-react'
import NativePanelShell, { NativeRow, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Tag { id: string; name: string; color?: string; resource_count?: number }

const PRESET_COLORS = ['#6366f1','#8b5cf6','#ec4899','#ef4444','#f97316','#eab308','#22c55e','#3b82f6','#14b8a6','#64748b']

export default function NativeTagsPanel() {
  const [tags, setTags] = useState<Tag[]>([])
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')
  const [creating, setCreating] = useState(false)
  const [newName, setNewName] = useState('')
  const [newColor, setNewColor] = useState(PRESET_COLORS[0])
  const [saving, setSaving] = useState(false)

  async function load() {
    const r = await fetch('/api/tags', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setTags(d.tags ?? []) }
    setLoading(false)
  }

  async function create() {
    if (!newName.trim()) return
    setSaving(true)
    await fetch('/api/tags', {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: newName.trim(), color: newColor }),
    })
    setSaving(false)
    setNewName('')
    setCreating(false)
    load()
  }

  async function remove(id: string) {
    await fetch(`/api/tags/${encodeURIComponent(id)}`, { method: 'DELETE', credentials: 'include' })
    load()
  }

  useEffect(() => { load() }, [])

  const filtered = tags.filter(t => t.name.toLowerCase().includes(search.toLowerCase()))

  const createForm = creating && (
    <div style={{ padding: '6px 10px', borderBottom: '1px solid var(--border-subtle)', background: 'var(--bg-elevated)' }}>
      <input
        autoFocus
        value={newName}
        onChange={e => setNewName(e.target.value)}
        onKeyDown={e => { if (e.key === 'Enter') create(); if (e.key === 'Escape') setCreating(false) }}
        placeholder="Tag name…"
        style={{ width: '100%', background: 'none', border: 'none', outline: 'none', fontSize: 11, color: 'var(--text-primary)', marginBottom: 4 }}
      />
      <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', alignItems: 'center' }}>
        {PRESET_COLORS.map(c => (
          <button key={c} onClick={() => setNewColor(c)} style={{
            width: 12, height: 12, borderRadius: '50%', background: c, border: 'none', cursor: 'pointer', flexShrink: 0,
            outline: newColor === c ? `2px solid ${c}` : 'none', outlineOffset: 1,
          }} />
        ))}
        <button
          onClick={create} disabled={!newName.trim() || saving}
          style={{ marginLeft: 'auto', fontSize: 9, padding: '2px 6px', borderRadius: 3, border: 'none', cursor: 'pointer', background: 'var(--accent-primary)', color: '#fff', opacity: saving ? 0.5 : 1 }}
        >{saving ? '…' : 'Create'}</button>
        <button onClick={() => setCreating(false)} style={{ fontSize: 9, padding: '2px 6px', borderRadius: 3, border: 'none', cursor: 'pointer', background: 'var(--bg-panel)', color: 'var(--text-muted)' }}>Cancel</button>
      </div>
    </div>
  )

  return (
    <NativePanelShell
      search={search} onSearch={setSearch} searchPlaceholder="Filter tags…"
      actions={
        <button onClick={() => setCreating(v => !v)} style={{
          display: 'flex', alignItems: 'center', gap: 4, fontSize: 10, padding: '3px 8px',
          borderRadius: 4, border: '1px solid var(--border-subtle)', cursor: 'pointer',
          background: creating ? 'var(--accent-primary)' : 'var(--bg-elevated)',
          color: creating ? '#fff' : 'var(--text-muted)',
        }}><Plus size={10} /> New tag</button>
      }
    >
      {createForm}
      {loading ? <LoadingState /> : filtered.length === 0 ? <EmptyState text="No tags" /> :
        filtered.map(t => (
          <NativeRow key={t.id}>
            <span style={{ width: 10, height: 10, borderRadius: 2, flexShrink: 0, background: t.color ?? 'var(--accent-primary)', display: 'inline-block' }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{t.name}</div>
              {t.resource_count != null && (
                <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{t.resource_count} resource{t.resource_count !== 1 ? 's' : ''}</div>
              )}
            </div>
            <IconBtn title="Delete" onClick={() => remove(t.id)} danger><Trash2 size={11} /></IconBtn>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
