import { useEffect, useState } from 'react'
import NativePanelShell, { NativeRow, EmptyState, LoadingState } from './NativePanelShell'

interface Tag { id: string; name: string; color?: string; resource_count?: number }

export default function NativeTagsPanel() {
  const [tags, setTags] = useState<Tag[]>([])
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')

  async function load() {
    const r = await fetch('/api/tags', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setTags(d.tags ?? []) }
    setLoading(false)
  }

  useEffect(() => { load() }, [])

  const filtered = tags.filter(t => t.name.toLowerCase().includes(search.toLowerCase()))

  return (
    <NativePanelShell search={search} onSearch={setSearch} searchPlaceholder="Filter tags…">
      {loading ? <LoadingState /> : filtered.length === 0 ? <EmptyState text="No tags" /> :
        filtered.map(t => (
          <NativeRow key={t.id}>
            <span style={{
              width: 10, height: 10, borderRadius: 2, flexShrink: 0,
              background: t.color ?? 'var(--accent-primary)',
              display: 'inline-block',
            }} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)' }}>{t.name}</div>
              {t.resource_count != null && (
                <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{t.resource_count} resource{t.resource_count !== 1 ? 's' : ''}</div>
              )}
            </div>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
