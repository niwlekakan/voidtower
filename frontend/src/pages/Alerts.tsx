import { useState, useEffect, useCallback } from 'react'
import { api } from '@/api/client'
import type { Alert } from '@/api/types'
import { notify } from '@/store/notifications'
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

  useEffect(() => { load() }, [load])

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

      {/* Alert list */}
      <div className="space-y-2">
        {alerts.map((alert) => (
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
                <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>{alert.message}</div>
                <div className="text-xs mt-2" style={{ color: 'var(--text-muted)' }}>
                  {fmtDate(alert.created_at)}
                  {alert.acknowledged_by && ` · Acknowledged by ${alert.acknowledged_by}`}
                </div>
              </div>

              <div className="flex items-center gap-2 shrink-0">
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
