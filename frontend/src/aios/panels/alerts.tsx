import { useEffect, useState } from 'react'
import { CheckCheck, XCircle } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Alert { id: string; title: string; severity: string; state: string; created_at: string }

const TABS = [{ id: 'active', label: 'Active' }, { id: 'resolved', label: 'Resolved' }]

function sevColor(s: string) {
  if (s === 'critical') return '#ef4444'
  if (s === 'warning') return '#f59e0b'
  return '#94a3b8'
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
        alerts.map(a => (
          <NativeRow key={a.id}>
            <StatusDot color={sevColor(a.severity)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{a.title}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{a.severity}</div>
            </div>
            {tab === 'active' && <>
              <IconBtn title="Acknowledge" onClick={() => act(a.id, 'ack')}><CheckCheck size={11} /></IconBtn>
              <IconBtn title="Resolve" onClick={() => act(a.id, 'resolve')} danger><XCircle size={11} /></IconBtn>
            </>}
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
