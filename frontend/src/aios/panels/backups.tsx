import { useEffect, useState } from 'react'
import { Play } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Backup { id: string; name: string; last_run?: string; status?: string; schedule?: string }

function statusColor(s?: string) {
  if (s === 'ok' || s === 'success') return '#22c55e'
  if (s === 'error' || s === 'failed') return '#ef4444'
  return '#94a3b8'
}

export default function NativeBackupsPanel() {
  const [backups, setBackups] = useState<Backup[]>([])
  const [loading, setLoading] = useState(true)
  const [running, setRunning] = useState<string | null>(null)

  async function load() {
    const r = await fetch('/api/backups', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setBackups(d.backups ?? []) }
    setLoading(false)
  }

  async function run(id: string) {
    setRunning(id)
    await fetch(`/api/backups/${id}/run`, { method: 'POST', credentials: 'include' })
    setRunning(null)
    load()
  }

  useEffect(() => { load() }, [])

  return (
    <NativePanelShell>
      {loading ? <LoadingState /> : backups.length === 0 ? <EmptyState text="No backup jobs" /> :
        backups.map(b => (
          <NativeRow key={b.id}>
            <StatusDot color={statusColor(b.status)} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{b.name}</div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)' }}>{b.last_run ?? b.schedule ?? 'never run'}</div>
            </div>
            <IconBtn title="Run now" onClick={() => run(b.id)}>
              <Play size={11} style={{ opacity: running === b.id ? 0.4 : 1 }} />
            </IconBtn>
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
