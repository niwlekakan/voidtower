import { useEffect, useState } from 'react'
import { CheckCheck, XCircle } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Alert {
  id: string
  title: string
  message?: string
  severity: string
  category?: string
  resource_type?: string
  state: string
  created_at: number | string
}

const TABS = [{ id: 'active', label: 'Active' }, { id: 'acknowledged', label: 'Ack' }, { id: 'resolved', label: 'Resolved' }]

function sevColor(s: string) {
  if (s === 'critical') return '#ef4444'
  if (s === 'warning')  return '#f59e0b'
  return '#94a3b8'
}

function rel(ts: number | string) {
  const ms = typeof ts === 'number' ? ts * 1000 : new Date(ts).getTime()
  const d = Date.now() - ms
  if (d < 60000) return `${Math.floor(d / 1000)}s ago`
  if (d < 3600000) return `${Math.floor(d / 60000)}m ago`
  if (d < 86400000) return `${Math.floor(d / 3600000)}h ago`
  return `${Math.floor(d / 86400000)}d ago`
}

export default function NativeAlertsPanel() {
  const [alerts, setAlerts] = useState<Alert[]>([])
  const [loading, setLoading] = useState(true)
  const [tab, setTab] = useState('active')

  async function load() {
    setLoading(true)
    const r = await fetch(`/api/alerts?state=${tab}`, { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setAlerts(d.alerts ?? []) }
    setLoading(false)
  }

  async function act(id: string, action: 'ack' | 'resolve') {
    await fetch(`/api/alerts/${id}/${action}`, { method: 'POST', credentials: 'include' })
    load()
  }

  useEffect(() => { load() }, [tab])

  return (
    <NativePanelShell tabs={TABS} activeTab={tab} onTabChange={setTab}>
      {loading ? <LoadingState /> : alerts.length === 0 ? <EmptyState text="No alerts" /> :
        alerts.map(a => {
          const meta = [a.category, a.resource_type, rel(a.created_at)].filter(Boolean).join(' · ')
          return (
            <NativeRow key={a.id} style={{ alignItems: 'flex-start' }}>
              <StatusDot color={sevColor(a.severity)} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{a.title}</div>
                {a.message && (
                  <div style={{
                    fontSize: 9, color: 'var(--text-secondary)', marginTop: 1,
                    display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical',
                    overflow: 'hidden',
                  }}>{a.message}</div>
                )}
                <div style={{ fontSize: 9, color: 'var(--text-muted)', marginTop: 1 }}>{meta}</div>
              </div>
              {tab === 'active' && <>
                <IconBtn title="Acknowledge" onClick={() => act(a.id, 'ack')}><CheckCheck size={11} /></IconBtn>
                <IconBtn title="Resolve" onClick={() => act(a.id, 'resolve')} danger><XCircle size={11} /></IconBtn>
              </>}
              {tab === 'acknowledged' && (
                <IconBtn title="Resolve" onClick={() => act(a.id, 'resolve')} danger><XCircle size={11} /></IconBtn>
              )}
            </NativeRow>
          )
        })
      }
    </NativePanelShell>
  )
}
