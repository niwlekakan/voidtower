import { useEffect, useRef, useState } from 'react'
import { Download, RefreshCw, Search } from 'lucide-react'
import { api } from '@/api/client'
import type { TimelineEvent } from '@/api/types'

const CATEGORIES: { id: string; label: string; color: string }[] = [
  { id: 'all',        label: 'All',        color: 'var(--text-muted)' },
  { id: 'auth',       label: 'Auth',       color: '#a78bfa' },
  { id: 'containers', label: 'Containers', color: '#38bdf8' },
  { id: 'services',   label: 'Services',   color: '#34d399' },
  { id: 'apps',       label: 'Apps',       color: '#fb923c' },
  { id: 'backups',    label: 'Backups',    color: '#fbbf24' },
  { id: 'secrets',    label: 'Secrets',    color: '#f472b6' },
  { id: 'networking', label: 'Networking', color: '#60a5fa' },
  { id: 'alerts',     label: 'Alerts',     color: '#f87171' },
  { id: 'files',      label: 'Files',      color: '#a3e635' },
  { id: 'system',     label: 'System',     color: 'var(--text-muted)' },
]

const CAT_COLOR: Record<string, string> = Object.fromEntries(CATEGORIES.map(c => [c.id, c.color]))

function relTime(ts: number) {
  const diff = Math.floor(Date.now() / 1000) - ts
  if (diff < 60) return `${diff}s ago`
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return new Date(ts * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' })
}

function EventRow({ e }: { e: TimelineEvent }) {
  const [open, setOpen] = useState(false)
  const color = CAT_COLOR[e.category] ?? 'var(--text-muted)'
  const failed = e.outcome === 'failure' || e.outcome === 'error'

  return (
    <div className="flex gap-3 group">
      {/* Timeline spine */}
      <div className="flex flex-col items-center flex-shrink-0 w-6">
        <div className="w-2.5 h-2.5 rounded-full mt-1 flex-shrink-0" style={{ background: failed ? 'var(--accent-error)' : color, boxShadow: `0 0 6px ${failed ? 'var(--accent-error)' : color}` }} />
        <div className="flex-1 w-px mt-1" style={{ background: 'var(--border-subtle)' }} />
      </div>

      {/* Content */}
      <div className="flex-1 pb-4 min-w-0">
        <button
          onClick={() => (e.details || e.resource_id) && setOpen(o => !o)}
          className="w-full text-left"
        >
          <div className="flex items-baseline gap-2 flex-wrap">
            <span className="text-xs font-semibold uppercase tracking-wider" style={{ color }}>{e.category}</span>
            {failed && <span className="text-xs px-1 rounded" style={{ background: 'color-mix(in srgb, var(--accent-error) 15%, transparent)', color: 'var(--accent-error)' }}>failed</span>}
            {e.source === 'odysseus' && <span className="text-xs px-1 rounded" style={{ background: 'color-mix(in srgb, #a78bfa 15%, transparent)', color: '#a78bfa' }}>odysseus</span>}
            <span className="flex-1 text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{e.action.replace(/_/g, ' ')}</span>
            <span className="text-xs tabular-nums" style={{ color: 'var(--text-muted)' }}>{relTime(e.timestamp)}</span>
          </div>
          <div className="text-xs mt-0.5 flex gap-2 flex-wrap" style={{ color: 'var(--text-muted)' }}>
            <span>by <span style={{ color: 'var(--text-secondary)' }}>{e.actor}</span></span>
            {e.resource_type && <span>· {e.resource_type}{e.resource_id ? ` ${e.resource_id.slice(0, 12)}` : ''}</span>}
            {e.ip_address && <span>· {e.ip_address}</span>}
          </div>
        </button>

        {open && (e.details || e.resource_id) && (
          <div className="mt-1.5 rounded px-2.5 py-2 text-xs font-mono" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
            {e.resource_id && <div>resource: {e.resource_id}</div>}
            {e.details && <div className="mt-0.5 whitespace-pre-wrap">{e.details}</div>}
          </div>
        )}
      </div>
    </div>
  )
}

function exportEvents(events: TimelineEvent[], format: 'json' | 'csv') {
  let content: string
  let mime: string
  let ext: string

  if (format === 'json') {
    content = JSON.stringify(events, null, 2)
    mime = 'application/json'
    ext = 'json'
  } else {
    const cols = ['id', 'timestamp', 'category', 'action', 'actor', 'actor_type', 'resource_type', 'resource_id', 'outcome', 'details', 'ip_address', 'source'] as const
    const escape = (v: unknown) => {
      if (v == null) return ''
      const s = String(v)
      return s.includes(',') || s.includes('"') || s.includes('\n') ? `"${s.replace(/"/g, '""')}"` : s
    }
    const rows = [cols.join(','), ...events.map(e => cols.map(c => escape(e[c])).join(','))]
    content = rows.join('\n')
    mime = 'text/csv'
    ext = 'csv'
  }

  const blob = new Blob([content], { type: mime })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `timeline-${new Date().toISOString().slice(0, 10)}.${ext}`
  a.click()
  URL.revokeObjectURL(url)
}

export default function TimelinePage() {
  const [events, setEvents] = useState<TimelineEvent[]>([])
  const [total, setTotal] = useState(0)
  const [category, setCategory] = useState('all')
  const [search, setSearch] = useState('')
  const [loading, setLoading] = useState(false)
  const [offset, setOffset] = useState(0)
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const LIMIT = 50

  const load = (off = 0, cat = category, q = search) => {
    setLoading(true)
    api.timeline.list({ limit: LIMIT, offset: off, category: cat !== 'all' ? cat : undefined, search: q || undefined })
      .then(r => {
        setEvents(prev => off === 0 ? r.events : [...prev, ...r.events])
        setTotal(r.total)
        setOffset(off)
      })
      .catch(() => {})
      .finally(() => setLoading(false))
  }

  // eslint-disable-next-line react-hooks/exhaustive-deps -- initial load only; onCategory/onSearch already trigger their own load() calls
  useEffect(() => { load(0, category, search) }, [])

  const onCategory = (cat: string) => { setCategory(cat); load(0, cat, search) }

  const onSearch = (q: string) => {
    setSearch(q)
    if (searchTimer.current) clearTimeout(searchTimer.current)
    searchTimer.current = setTimeout(() => load(0, category, q), 300)
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between gap-4 flex-wrap">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Timeline</h1>
          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
            {total} event{total !== 1 ? 's' : ''}
            {category !== 'all' ? ` in ${category}` : ''}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <div className="flex items-center rounded overflow-hidden" style={{ border: '1px solid var(--border-subtle)' }}>
            <button onClick={() => exportEvents(events, 'json')} disabled={events.length === 0} title="Export as JSON"
              className="flex items-center gap-1.5 px-3 py-1.5 text-sm hover:opacity-80 disabled:opacity-40"
              style={{ background: 'var(--bg-panel)', color: 'var(--text-secondary)', borderRight: '1px solid var(--border-subtle)' }}>
              <Download size={13} /> JSON
            </button>
            <button onClick={() => exportEvents(events, 'csv')} disabled={events.length === 0} title="Export as CSV"
              className="flex items-center gap-1.5 px-3 py-1.5 text-sm hover:opacity-80 disabled:opacity-40"
              style={{ background: 'var(--bg-panel)', color: 'var(--text-secondary)' }}>
              CSV
            </button>
          </div>
          <button onClick={() => load(0)} disabled={loading} className="flex items-center gap-1.5 px-3 py-1.5 rounded text-sm hover:opacity-80 disabled:opacity-50" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
            <RefreshCw size={13} className={loading ? 'animate-spin' : ''} /> Refresh
          </button>
        </div>
      </div>

      {/* Search */}
      <div className="relative">
        <Search size={13} className="absolute left-3 top-1/2 -translate-y-1/2" style={{ color: 'var(--text-muted)' }} />
        <input value={search} onChange={e => onSearch(e.target.value)} placeholder="Search actions, resources, details…"
          className="w-full rounded pl-8 pr-3 py-2 text-sm outline-none"
          style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} />
      </div>

      {/* Category chips */}
      <div className="flex gap-1.5 flex-wrap">
        {CATEGORIES.map(c => (
          <button key={c.id} onClick={() => onCategory(c.id)}
            className="px-2.5 py-1 rounded-full text-xs transition-opacity hover:opacity-80"
            style={{
              background: category === c.id ? 'color-mix(in srgb, ' + c.color + ' 20%, transparent)' : 'var(--bg-panel)',
              border: `1px solid ${category === c.id ? c.color : 'var(--border-subtle)'}`,
              color: category === c.id ? c.color : 'var(--text-muted)',
            }}>
            {c.label}
          </button>
        ))}
      </div>

      {/* Timeline */}
      <div className="card pt-3 px-4 pb-2">
        {events.length === 0 && !loading && (
          <div className="py-16 text-center text-sm" style={{ color: 'var(--text-muted)' }}>No events found.</div>
        )}
        {events.map(e => <EventRow key={e.id} e={e} />)}

        {events.length < total && (
          <button onClick={() => load(offset + LIMIT)} disabled={loading}
            className="w-full py-2 text-sm rounded mt-2 mb-2 disabled:opacity-50 hover:opacity-80"
            style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
            {loading ? 'Loading…' : `Load more (${total - events.length} remaining)`}
          </button>
        )}
      </div>
    </div>
  )
}
