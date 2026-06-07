import { useEffect, useState } from 'react'
import { Play } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Automation { id: string; name: string; enabled: boolean; last_run?: string; schedule?: string }

export default function NativeAutomationPanel() {
  const [items, setItems] = useState<Automation[]>([])
  const [loading, setLoading] = useState(true)

  async function load() {
    const r = await fetch('/api/automation', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setItems(d.automations ?? d ?? []) }
    setLoading(false)
  }

  async function run(id: string) {
    await fetch(`/api/automation/${id}/run`, { method: 'POST', credentials: 'include' })
    load()
  }

  useEffect(() => { load() }, [])

  return (
    <NativePanelShell>
      {loading ? <LoadingState /> : items.length === 0 ? <EmptyState text="No automations" /> :
        items.map(a => (
          <NativeRow key={a.id}>
            <StatusDot color={a.enabled ? '#22c55e' : '#94a3b8'} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{a.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{a.schedule ?? (a.last_run ? `last: ${a.last_run}` : 'manual')}</div>
            </div>
            <IconBtn title="Run now" onClick={() => run(a.id)}><Play size={11} /></IconBtn>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
