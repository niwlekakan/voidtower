import { useState, useEffect, useCallback } from 'react'
import { api } from '@/api/client'
import type { AuditEntry } from '@/api/types'
import { notify } from '@/store/notifications'
import Button from '@/components/ui/Button'

function fmt(ts: number): string {
  return new Date(ts * 1000).toLocaleString()
}

const outcomeColor: Record<string, string> = {
  success: 'var(--accent-success)',
  failure: 'var(--accent-danger)',
  denied:  'var(--accent-warning)',
}

export default function AuditPage() {
  const [entries, setEntries] = useState<AuditEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [offset, setOffset] = useState(0)
  const limit = 50

  const load = useCallback(async (off: number) => {
    setLoading(true)
    try {
      const data = await api.audit.list(limit, off)
      setEntries(data.entries)
    } catch {
      notify.error('Failed to load audit log')
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => { load(0) }, [load])

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Audit Log</h1>
        <Button size="sm" onClick={() => { setOffset(0); load(0) }} loading={loading}>Refresh</Button>
      </div>

      <div className="panel overflow-hidden">
        <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead>
            <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
              {['Time', 'Actor', 'Action', 'Resource', 'Outcome', 'IP'].map((h) => (
                <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {entries.map((e) => (
              <tr key={e.id} style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                <td className="px-4 py-2 font-mono" style={{ color: 'var(--text-muted)' }}>{fmt(e.timestamp)}</td>
                <td className="px-4 py-2" style={{ color: 'var(--text-secondary)' }}>{e.actor_type}</td>
                <td className="px-4 py-2" style={{ color: 'var(--text-primary)' }}>{e.action}</td>
                <td className="px-4 py-2" style={{ color: 'var(--text-secondary)' }}>
                  {e.resource_type ? `${e.resource_type}${e.resource_id ? `:${e.resource_id}` : ''}` : '—'}
                </td>
                <td className="px-4 py-2">
                  <span style={{ color: outcomeColor[e.outcome] ?? 'var(--text-secondary)' }}>{e.outcome}</span>
                </td>
                <td className="px-4 py-2 font-mono" style={{ color: 'var(--text-muted)' }}>{e.ip_address ?? '—'}</td>
              </tr>
            ))}
            {!loading && entries.length === 0 && (
              <tr><td colSpan={6} className="px-4 py-8 text-center" style={{ color: 'var(--text-muted)' }}>No entries.</td></tr>
            )}
          </tbody>
        </table>
        </div>
      </div>

      <div className="flex items-center justify-between text-xs" style={{ color: 'var(--text-muted)' }}>
        <span>Showing {offset + 1}–{offset + entries.length}</span>
        <div className="flex gap-2">
          <Button size="sm" variant="ghost" disabled={offset === 0} onClick={() => { const o = Math.max(0, offset - limit); setOffset(o); load(o) }}>
            Previous
          </Button>
          <Button size="sm" variant="ghost" disabled={entries.length < limit} onClick={() => { const o = offset + limit; setOffset(o); load(o) }}>
            Next
          </Button>
        </div>
      </div>
    </div>
  )
}
