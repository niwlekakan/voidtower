import { useEffect, useState } from 'react'
import { LogOut } from 'lucide-react'
import NativePanelShell, { NativeRow, StatusDot, IconBtn, EmptyState, LoadingState } from './NativePanelShell'

interface Session { id: string; user?: string; ip?: string; user_agent?: string; created_at: string; is_current?: boolean }

function rel(ts: string) {
  const d = Date.now() - new Date(ts).getTime()
  if (d < 3600000) return `${Math.floor(d / 60000)}m ago`
  if (d < 86400000) return `${Math.floor(d / 3600000)}h ago`
  return `${Math.floor(d / 86400000)}d ago`
}

export default function NativeSecurityPanel() {
  const [sessions, setSessions] = useState<Session[]>([])
  const [loading, setLoading] = useState(true)

  async function load() {
    const r = await fetch('/api/security/sessions', { credentials: 'include' })
    if (r.ok) { const d = await r.json(); setSessions(d.sessions ?? []) }
    setLoading(false)
  }

  async function revoke(id: string) {
    await fetch(`/api/security/sessions/${id}`, { method: 'DELETE', credentials: 'include' })
    load()
  }

  useEffect(() => { load() }, [])

  return (
    <NativePanelShell>
      {loading ? <LoadingState /> : sessions.length === 0 ? <EmptyState text="No active sessions" /> :
        sessions.map(s => (
          <NativeRow key={s.id}>
            <StatusDot color={s.is_current ? '#22c55e' : '#94a3b8'} />
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontSize: 11, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {s.ip ?? 'unknown'}{s.is_current ? ' (this session)' : ''}
              </div>
              <div style={{ fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                {rel(s.created_at)}{s.user_agent ? ` · ${s.user_agent.slice(0, 30)}` : ''}
              </div>
            </div>
            {!s.is_current && (
              <IconBtn title="Revoke session" onClick={() => revoke(s.id)} danger><LogOut size={11} /></IconBtn>
            )}
          </NativeRow>
        ))
      }
    </NativePanelShell>
  )
}
