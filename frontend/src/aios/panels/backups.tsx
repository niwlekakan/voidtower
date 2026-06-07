import { useEffect, useState } from 'react'
import { Play, ShieldCheck, FlaskConical, Trash2 } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Backup {
  id: string; name: string; last_run?: string; status?: string; schedule?: string
  source_path?: string; repo_path?: string; confidence?: number; retention_days?: number
}

function statusColor(s?: string) {
  if (s === 'ok' || s === 'success') return '#22c55e'
  if (s === 'error' || s === 'failed') return '#ef4444'
  return '#94a3b8'
}

function confidenceLabel(c?: number): { text: string; color: string } {
  if (c === undefined || c === null) return { text: 'Unknown', color: '#94a3b8' }
  if (c >= 80) return { text: 'Good',  color: '#22c55e' }
  if (c >= 50) return { text: 'Fair',  color: '#f59e0b' }
  return          { text: 'Low',   color: '#ef4444' }
}

function truncate(s: string, n: number) {
  return s.length > n ? '…' + s.slice(-(n - 1)) : s
}

export default function NativeBackupsPanel() {
  const [backups, setBackups] = useState<Backup[]>([])
  const [loading, setLoading] = useState(true)
  const [busy, setBusy] = useState<Record<string, string>>({})

  async function load() {
    const r = await fetch('/api/backups', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setBackups(d.backups ?? d.configs ?? []) }
    setLoading(false)
  }

  async function act(id: string, label: string, path: string, method = 'POST') {
    setBusy(b => ({ ...b, [id]: label }))
    await fetch(path, { method, credentials: 'include' })
    setBusy(b => { const n = { ...b }; delete n[id]; return n })
    load()
  }

  useEffect(() => { load() }, [])

  return (
    <NativePanelShell>
      {loading ? <LoadingState /> : backups.length === 0 ? <EmptyState text="No backup jobs" /> :
        backups.map(b => {
          const conf = confidenceLabel(b.confidence)
          const sub = b.source_path ?? b.repo_path
          const isBusy = !!busy[b.id]
          return (
            <NativeRow key={b.id}>
              <StatusDot color={statusColor(b.status)} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{b.name}</div>
                <div style={{ fontSize: 9, color: 'var(--text-muted)', display: 'flex', gap: 6, alignItems: 'center' }}>
                  <span>{b.last_run ?? b.schedule ?? 'never run'}</span>
                  {sub && <span style={{ fontFamily: 'monospace', opacity: 0.8 }}>{truncate(sub, 30)}</span>}
                  <span style={{ color: conf.color, fontWeight: 600 }}>{conf.text}</span>
                </div>
              </div>
              <IconBtn title={busy[b.id] === 'Run' ? 'Running…' : 'Run now'} onClick={() => act(b.id, 'Run', `/api/backups/${b.id}/run`)}>
                <Play size={11} style={{ opacity: isBusy ? 0.4 : 1 }} />
              </IconBtn>
              <IconBtn title={busy[b.id] === 'Check' ? 'Checking…' : 'Check integrity'} onClick={() => act(b.id, 'Check', `/api/backups/${b.id}/check`)}>
                <ShieldCheck size={11} style={{ opacity: isBusy ? 0.4 : 1 }} />
              </IconBtn>
              <IconBtn title={busy[b.id] === 'Test' ? 'Testing…' : 'Restore test'} onClick={() => act(b.id, 'Test', `/api/backups/${b.id}/restore-test`)}>
                <FlaskConical size={11} style={{ opacity: isBusy ? 0.4 : 1 }} />
              </IconBtn>
              <IconBtn title="Delete" danger onClick={() => act(b.id, 'Del', `/api/backups/${b.id}`, 'DELETE')}>
                <Trash2 size={11} />
              </IconBtn>
            </NativeRow>
          )
        })
      }
    </NativePanelShell>
  )
}
