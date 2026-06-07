import { useEffect, useState } from 'react'
import NativePanelShell, { NativeRow, StatusDot, EmptyState, LoadingState } from './NativePanelShell'

interface Event {
  id: string
  action: string
  category?: string
  outcome?: string
  resource_type?: string
  resource_name?: string
  actor?: string
  created_at: string
}

const CAT_COLORS: Record<string, string> = {
  auth: '#a78bfa', containers: '#38bdf8', services: '#34d399',
  apps: '#fb923c', backups: '#fbbf24', secrets: '#f472b6',
  networking: '#60a5fa', alerts: '#f87171', files: '#a3e635', system: '#94a3b8',
}

function outcomeColor(outcome?: string) {
  if (outcome === 'success') return '#22c55e'
  if (outcome === 'failure' || outcome === 'error') return '#ef4444'
  if (outcome === 'warning') return '#f59e0b'
  return '#94a3b8'
}

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
  const [search, setSearch] = useState('')

  async function load(q = '') {
    setLoading(true)
    const qs = q ? `&search=${encodeURIComponent(q)}` : ''
    const r = await fetch(`/api/timeline?limit=30${qs}`, { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setEvents(d.events ?? []) }
    setLoading(false)
  }

  useEffect(() => { load() }, [])

  function onSearch(v: string) {
    setSearch(v)
    load(v)
  }

  return (
    <NativePanelShell search={search} onSearch={onSearch} searchPlaceholder="Search events…">
      {loading ? <LoadingState /> : events.length === 0 ? <EmptyState text="No events" /> :
        events.map(e => {
          const catColor = CAT_COLORS[e.category ?? ''] ?? '#94a3b8'
          const subParts = [
            e.outcome,
            e.actor ? `by ${e.actor}` : '',
            e.resource_type ?? '',
            rel(e.created_at),
          ].filter(Boolean).join(' · ')
          return (
            <NativeRow key={e.id}>
              <StatusDot color={outcomeColor(e.outcome)} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                  {e.action.replace(/_/g, ' ')}{e.resource_name ? ` · ${e.resource_name}` : ''}
                </div>
                <div style={{ display: 'flex', alignItems: 'center', gap: 4, marginTop: 1, flexWrap: 'nowrap', overflow: 'hidden' }}>
                  {e.category && (
                    <span style={{
                      fontSize: 8, padding: '0 4px', borderRadius: 3, lineHeight: '14px',
                      background: catColor + '28', color: catColor, flexShrink: 0,
                    }}>{e.category}</span>
                  )}
                  <span style={{ fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{subParts}</span>
                </div>
              </div>
            </NativeRow>
          )
        })
      }
    </NativePanelShell>
  )
}
