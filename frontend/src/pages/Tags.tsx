import { useState, useEffect, useCallback } from 'react'
import { api } from '@/api/client'
import type { Tag } from '@/api/types'
import { notify } from '@/store/notifications'
import Button from '@/components/ui/Button'
import { TagPill } from '@/components/ui/TagPill'

const PRESET_COLORS = [
  '#6366f1', '#8b5cf6', '#ec4899', '#ef4444', '#f97316',
  '#eab308', '#22c55e', '#14b8a6', '#3b82f6', '#64748b',
]

function ColorDot({ color, selected, onClick }: { color: string; selected: boolean; onClick: () => void }) {
  return (
    <button
      onClick={onClick}
      title={color}
      style={{
        width: 20, height: 20, borderRadius: '50%', background: color, border: 'none', cursor: 'pointer',
        outline: selected ? `2px solid ${color}` : 'none', outlineOffset: 2,
        transform: selected ? 'scale(1.2)' : 'scale(1)', transition: 'transform 0.1s',
      }}
    />
  )
}

export default function TagsPage() {
  const [tags, setTags] = useState<Tag[]>([])
  const [loading, setLoading] = useState(true)
  const [newName, setNewName] = useState('')
  const [newColor, setNewColor] = useState(PRESET_COLORS[0])
  const [creating, setCreating] = useState(false)
  const [editing, setEditing] = useState<string | null>(null)
  const [editName, setEditName] = useState('')
  const [editColor, setEditColor] = useState('')

  const load = useCallback(async () => {
    try { setTags(await api.tags.list()) }
    catch { notify.error('Failed to load tags') }
    finally { setLoading(false) }
  }, [])

  useEffect(() => { load() }, [load])

  const create = async () => {
    if (!newName.trim()) return
    setCreating(true)
    try {
      await api.tags.create(newName.trim(), newColor)
      setNewName('')
      load()
    } catch (e: unknown) {
      notify.error(String(e).includes('Conflict') ? `Tag "${newName}" already exists` : 'Failed to create tag')
    } finally { setCreating(false) }
  }

  const startEdit = (tag: Tag) => {
    setEditing(tag.id)
    setEditName(tag.name)
    setEditColor(tag.color)
  }

  const saveEdit = async (id: string) => {
    try {
      await api.tags.update(id, { name: editName.trim(), color: editColor })
      setEditing(null)
      load()
    } catch { notify.error('Failed to update tag') }
  }

  const remove = async (id: string, name: string) => {
    if (!confirm(`Delete tag "${name}"? It will be removed from all resources.`)) return
    try { await api.tags.delete(id); load() }
    catch { notify.error('Failed to delete tag') }
  }

  return (
    <div className="space-y-4 max-w-2xl">
      <div>
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Tags</h1>
        <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
          Label services, containers, and proxies for filtering and grouping.
        </p>
      </div>

      {/* Create */}
      <div className="card">
        <h3 className="text-sm font-semibold mb-3" style={{ color: 'var(--text-primary)' }}>New tag</h3>
        <div style={{ display: 'flex', gap: 10, alignItems: 'flex-end', flexWrap: 'wrap' }}>
          <div style={{ flex: '1 1 160px' }}>
            <label style={{ fontSize: 12, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Name</label>
            <input
              value={newName}
              onChange={e => setNewName(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && create()}
              placeholder="e.g. production"
              style={{ width: '100%', background: 'var(--bg-input)', border: '1px solid var(--border-subtle)', borderRadius: 6, color: 'var(--text-primary)', padding: '6px 10px', fontSize: 13, outline: 'none', boxSizing: 'border-box' }}
            />
          </div>
          <div>
            <label style={{ fontSize: 12, color: 'var(--text-muted)', display: 'block', marginBottom: 4 }}>Color</label>
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
              {PRESET_COLORS.map(c => <ColorDot key={c} color={c} selected={newColor === c} onClick={() => setNewColor(c)} />)}
            </div>
          </div>
          <Button onClick={create} loading={creating} disabled={!newName.trim()}>Create</Button>
        </div>
        {newName && (
          <div style={{ marginTop: 10 }}>
            <span style={{ fontSize: 12, color: 'var(--text-muted)', marginRight: 8 }}>Preview:</span>
            <TagPill tag={{ id: '', name: newName, color: newColor, created_at: 0 }} />
          </div>
        )}
      </div>

      {/* List */}
      {loading ? (
        <p className="text-sm" style={{ color: 'var(--text-muted)' }}>Loading…</p>
      ) : tags.length === 0 ? (
        <p className="text-sm" style={{ color: 'var(--text-muted)' }}>No tags yet. Create one above.</p>
      ) : (
        <div className="space-y-1.5">
          {tags.map(tag => (
            <div key={tag.id} className="card flex items-center gap-2.5">
              {editing === tag.id ? (
                <>
                  <div style={{ display: 'flex', gap: 5 }}>
                    {PRESET_COLORS.map(c => <ColorDot key={c} color={c} selected={editColor === c} onClick={() => setEditColor(c)} />)}
                  </div>
                  <input
                    value={editName}
                    onChange={e => setEditName(e.target.value)}
                    onKeyDown={e => { if (e.key === 'Enter') saveEdit(tag.id); if (e.key === 'Escape') setEditing(null) }}
                    autoFocus
                    style={{ flex: 1, background: 'var(--bg-input)', border: '1px solid var(--border-subtle)', borderRadius: 6, color: 'var(--text-primary)', padding: '4px 8px', fontSize: 13, outline: 'none' }}
                  />
                  <Button size="sm" onClick={() => saveEdit(tag.id)}>Save</Button>
                  <Button size="sm" variant="secondary" onClick={() => setEditing(null)}>Cancel</Button>
                </>
              ) : (
                <>
                  <TagPill tag={tag} />
                  <div style={{ flex: 1 }} />
                  <button onClick={() => startEdit(tag)} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', fontSize: 12, padding: '2px 6px' }}>Edit</button>
                  <button onClick={() => remove(tag.id, tag.name)} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent-danger)', fontSize: 12, padding: '2px 6px' }}>Delete</button>
                </>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
