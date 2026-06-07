import { useEffect, useState } from 'react'
import NativePanelShell, { NativeRow, EmptyState, LoadingState } from './NativePanelShell'

interface Event { id: string; action: string; resource_type?: string; resource_name?: string; actor?: string; created_at: string }

function rel(ts: string) {
  const d = Date.now() - new Date(ts).getTime()
  if (d < 60000) return `${Math.floor(d / 1000)}s ago`
  if (d < 3600000) return `${Math.floor(d / 60000)}m ago`
  if (d < 86400000) return `${Math.floor(d / 3600000)}h ago`
  return `${Math.floor(d / 86400000)}d ago`
}

export default function NativeTimelinePanel() {
  const [events, setEvents] = useState<Event[]>([])
  const [loading, setLoading] = useState(true)

  async function load() {
    const r = await fetch('/api/timeline?limit=30', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setEvents(d.events ?? []) }
    setLoading(false)
  }

  useEffect(() => { load() }, [])

  return (
    <NativePanelShell>
      {loading ? <LoadingState /> : events.length === 0 ? <EmptyState text="No events" /> :
        events.map(e => (
          <NativeRow key={e.id}>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {e.action}{e.resource_name ? ` · ${e.resource_name}` : ''}
              </div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{rel(e.created_at)}{e.actor ? ` · ${e.actor}` : ''}{e.resource_type ? ` · ${e.resource_type}` : ''}</div>
            </div>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
