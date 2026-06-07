import { useEffect, useState } from 'react'
import { Play, Square, RotateCcw } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Service {
  name: string
  display_name?: string
  active: string
  enabled: boolean
}

function statusColor(active: string) {
  if (active === 'active') return '#22c55e'
  if (active === 'failed') return '#ef4444'
  return '#94a3b8'
}

export default function NativeServicesPanel() {
  const [services, setServices] = useState<Service[]>([])
  const [loading, setLoading] = useState(true)
  const [search, setSearch] = useState('')

  async function load() {
    const r = await fetch('/api/services', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setServices(d.services ?? []) }
    setLoading(false)
  }

  async function act(name: string, action: string) {
    await fetch(`/api/services/${encodeURIComponent(name)}/action`, {
      method: 'POST', credentials: 'include',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ action }),
    })
    load()
  }

  useEffect(() => { load() }, [])

  const filtered = services.filter(s =>
    s.name.toLowerCase().includes(search.toLowerCase()) ||
    (s.display_name ?? '').toLowerCase().includes(search.toLowerCase())
  )

  return (
    <NativePanelShell search={search} onSearch={setSearch} searchPlaceholder="Filter services…">
      {loading ? <LoadingState /> : filtered.length === 0 ? <EmptyState text="No services" /> :
        filtered.map(s => (
          <NativeRow key={s.name}>
            <StatusDot color={statusColor(s.active)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{s.display_name ?? s.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{s.active}{s.enabled ? '' : ' · disabled'}</div>
            </div>
            <IconBtn title="Start"   onClick={() => act(s.name, 'start')}><Play size={11} /></IconBtn>
            <IconBtn title="Stop"    onClick={() => act(s.name, 'stop')}><Square size={11} /></IconBtn>
            <IconBtn title="Restart" onClick={() => act(s.name, 'restart')}><RotateCcw size={11} /></IconBtn>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
