import { useEffect, useState } from 'react'
import { Play, Square, RotateCcw, Trash2 } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Container { id: string; name: string; image: string; state: string }

function stateColor(s: string) {
  return s === 'running' ? '#22c55e' : s === 'paused' ? '#f59e0b' : '#94a3b8'
}

export default function NativeContainersPanel() {
  const [containers, setContainers] = useState<Container[]>([])
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')

  async function load() {
    const r = await fetch('/api/containers', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setContainers(d.containers ?? []) }
    setLoading(false)
  }

  async function act(id: string, action: string) {
    await fetch(`/api/containers/${encodeURIComponent(id)}/action`, {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ action }),
    })
    load()
  }

  useEffect(() => { load() }, [])

  const filtered = containers.filter(c =>
    c.name.toLowerCase().includes(search.toLowerCase()) ||
    c.image.toLowerCase().includes(search.toLowerCase())
  )

  return (
    <NativePanelShell search={search} onSearch={setSearch} searchPlaceholder="Filter containers…">
      {loading ? <LoadingState /> : filtered.length === 0 ? <EmptyState text="No containers" /> :
        filtered.map(c => (
          <NativeRow key={c.id}>
            <StatusDot color={stateColor(c.state)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{c.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{c.image}</div>
            </div>
            <IconBtn title="Start"   onClick={() => act(c.id, 'start')}><Play size={11} /></IconBtn>
            <IconBtn title="Stop"    onClick={() => act(c.id, 'stop')}><Square size={11} /></IconBtn>
            <IconBtn title="Restart" onClick={() => act(c.id, 'restart')}><RotateCcw size={11} /></IconBtn>
            <IconBtn title="Remove"  onClick={() => act(c.id, 'remove')} danger><Trash2 size={11} /></IconBtn>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
