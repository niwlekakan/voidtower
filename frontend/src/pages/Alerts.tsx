import { useState, useEffect, useCallback } from 'react'
import { Tag as TagIcon } from 'lucide-react'
import { api } from '@/api/client'
import type { Alert, Tag, TagMap } from '@/api/types'
import { notify } from '@/store/notifications'
import { useFiltersStore } from '@/store/filters'
import { TagPill, TagPopover } from '@/components/ui/TagPill'
import Button from '@/components/ui/Button'
import SendToOdysseus from '@/components/ui/SendToOdysseus'

function severityColor(severity: string): string {
  switch (severity) {
    case 'critical': return 'var(--accent-danger)'
    case 'warning':  return 'var(--accent-warning)'
    case 'info':     return 'var(--accent-secondary)'
    default:         return 'var(--text-muted)'
  }
}

function fmtDate(ts: number) {
  return new Date(ts * 1000).toLocaleString()
}

export default function AlertsPage() {
  const [alerts, setAlerts] = useState<Alert[]>([])
  const [total, setTotal] = useState(0)
  const [loading, setLoading] = useState(true)
  const [stateFilter, setStateFilter] = useState<'active' | 'acknowledged' | 'resolved'>('active')
  const [actionLoading, setActionLoading] = useState<string | null>(null)
  const [allTags, setAllTags] = useState<Tag[]>([])
  const [tagMap, setTagMap] = useState<TagMap>({})
  const [popover, setPopover] = useState<string | null>(null)
  const globalTag = useFiltersStore((s) => s.globalTag)

  const load = useCallback(async () => {
    setLoading(true)
    try {
      const data = await api.alerts.list(stateFilter)
      setAlerts(data.alerts)
      setTotal(data.total)
    } catch {
      notify.error('Failed to load alerts')
    } finally {
      setLoading(false)
    }
  }, [stateFilter])

  const loadTags = useCallback(async () => {
    try {
      const [tags, map] = await Promise.all([api.tags.list(), api.tags.map('alert')])
      setAllTags(tags)
      setTagMap(map)
    } catch { /* empty */ }
  }, [])

  useEffect(() => { load(); loadTags() }, [load, loadTags])

  const doAck = async (id: string) => {
    setActionLoading(id + '-ack')
    try {
      await api.alerts.acknowledge(id)
      await load()
    } catch {
      notify.error('Failed to acknowledge alert')
    } finally {
      setActionLoading(null)
    }
  }

  const doResolve = async (id: string) => {
    setActionLoading(id + '-resolve')
    try {
      await api.alerts.resolve(id)
      await load()
    } catch {
      notify.error('Failed to resolve alert')
    } finally {
      setActionLoading(null)
    }
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Alerts</h1>
          <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>{total} {stateFilter} alert{total !== 1 ? 's' : ''}</p>
        </div>
        <Button size="sm" onClick={load} loading={loading}>Refresh</Button>
      </div>

      {/* State filter */}
      <div className="flex gap-1 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
        {(['active', 'acknowledged', 'resolved'] as const).map((s) => (
          <button
            key={s}
            onClick={() => setStateFilter(s)}
            className="px-4 py-2 text-xs capitalize transition-colors border-b-2 -mb-px"
            style={{
              color: stateFilter === s ? 'var(--accent-primary)' : 'var(--text-muted)',
              borderColor: stateFilter === s ? 'var(--accent-primary)' : 'transparent',
            }}
          >
            {s}
          </button>
        ))}
      </div>

      {/* Alert list — works at all widths; container query tweaks action placement at ≤700px */}
      <div className="alerts-cards space-y-2">
        {(globalTag ? alerts.filter(a => (tagMap[a.id] || []).some(t => t.id === globalTag)) : alerts).map((alert) => (
          <div
            key={alert.id}
            className="panel p-4"
            style={{ borderLeft: `3px solid ${severityColor(alert.severity)}` }}
          >
            <div className="flex items-start justify-between gap-4">
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2 mb-1">
                  <span
                    className="text-xs font-medium uppercase tracking-wider"
                    style={{ color: severityColor(alert.severity) }}
                  >
                    {alert.severity}
                  </span>
                  <span className="text-xs" style={{ color: 'var(--text-muted)' }}>{alert.category}</span>
                  {alert.resource_type && (
                    <span className="text-xs font-mono" style={{ color: 'var(--text-muted)' }}>
                      {alert.resource_type}{alert.resource_id ? `:${alert.resource_id}` : ''}
                    </span>
                  )}
                </div>
                <div className="text-sm font-medium mb-0.5" style={{ color: 'var(--text-primary)' }}>{alert.title}</div>
                <div className="alert-message text-xs" style={{ color: 'var(--text-secondary)' }}>{alert.message}</div>
                <div className="text-xs mt-2" style={{ color: 'var(--text-muted)' }}>
                  {fmtDate(alert.created_at)}
                  {alert.acknowledged_by && ` · Acknowledged by ${alert.acknowledged_by}`}
                </div>
                <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 4, alignItems: 'center', position: 'relative' }}>
                  {(tagMap[alert.id] || []).map(t => <TagPill key={t.id} tag={t} />)}
                  <button onClick={() => setPopover(popover === alert.id ? null : alert.id)} style={{
                    background: 'none', border: '1px dashed var(--border-subtle)', borderRadius: 10,
                    cursor: 'pointer', color: 'var(--text-muted)', fontSize: 11, padding: '0px 6px', lineHeight: '18px',
                  }}><TagIcon size={10} style={{ display: 'inline', verticalAlign: 'middle' }} /></button>
                  {popover === alert.id && (
                    <TagPopover resourceType="alert" resourceId={alert.id} allTags={allTags} assigned={tagMap[alert.id] || []} onClose={() => { setPopover(null); loadTags() }} />
                  )}
                </div>
                {/* Compact actions — shown at narrow width via container query */}
                <div className="alert-actions-compact" style={{ display: 'none', gap: 6, marginTop: 8, flexWrap: 'wrap' }}>
                  {stateFilter === 'active' && (
                    <>
                      <Button size="sm" variant="ghost" loading={actionLoading === alert.id + '-ack'} onClick={() => doAck(alert.id)}>Ack</Button>
                      <Button size="sm" variant="ghost" loading={actionLoading === alert.id + '-resolve'} onClick={() => doResolve(alert.id)}>Resolve</Button>
                    </>
                  )}
                  {stateFilter === 'acknowledged' && (
                    <Button size="sm" variant="ghost" loading={actionLoading === alert.id + '-resolve'} onClick={() => doResolve(alert.id)}>Resolve</Button>
                  )}
                </div>
              </div>

              {/* Wide actions — hidden at narrow width via container query */}
              <div className="alert-actions-wide flex items-center gap-2 shrink-0">
                <SendToOdysseus
                  context={`Alert [${alert.severity.toUpperCase()}] ${alert.title}\n${alert.message}\nCategory: ${alert.category}${alert.resource_type ? `\nResource: ${alert.resource_type}${alert.resource_id ? `:${alert.resource_id}` : ''}` : ''}`}
                  label="Odysseus"
                />
                {stateFilter === 'active' && (
                  <>
                    <Button
                      size="sm"
                      variant="ghost"
                      loading={actionLoading === alert.id + '-ack'}
                      onClick={() => doAck(alert.id)}
                    >
                      Ack
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      loading={actionLoading === alert.id + '-resolve'}
                      onClick={() => doResolve(alert.id)}
                    >
                      Resolve
                    </Button>
                  </>
                )}
                {stateFilter === 'acknowledged' && (
                  <Button
                    size="sm"
                    variant="ghost"
                    loading={actionLoading === alert.id + '-resolve'}
                    onClick={() => doResolve(alert.id)}
                  >
                    Resolve
                  </Button>
                )}
              </div>
            </div>
          </div>
        ))}

        {!loading && alerts.length === 0 && (
          <div className="panel p-10 text-center" style={{ color: 'var(--text-muted)' }}>
            {stateFilter === 'active' ? 'No active alerts.' : `No ${stateFilter} alerts.`}
          </div>
        )}
      </div>
    </div>
  )
}
