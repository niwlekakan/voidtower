import { useState, useEffect, useCallback } from 'react'
import { Monitor, Trash2, LogOut } from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import { useAuthStore } from '@/store/auth'
import { notify } from '@/store/notifications'
import type { SessionInfo } from '@/api/types'
import Button from '@/components/ui/Button'

function formatDate(ts: number) {
  return new Date(ts * 1000).toLocaleString(undefined, {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  })
}

function parseUA(ua: string | null) {
  if (!ua) return 'Unknown client'
  if (ua.includes('Firefox')) return 'Firefox'
  if (ua.includes('Chrome')) return 'Chrome'
  if (ua.includes('Safari')) return 'Safari'
  if (ua.includes('curl')) return 'curl'
  return ua.slice(0, 40)
}

export default function SecurityPage() {
  const user = useAuthStore((s) => s.user)
  const isAdmin = user?.role === 'owner' || user?.role === 'admin'
  const [sessions, setSessions] = useState<SessionInfo[]>([])
  const [currentId, setCurrentId] = useState('')
  const [loading, setLoading] = useState(true)
  const [revoking, setRevoking] = useState<string | null>(null)

  const refresh = useCallback(() => {
    setLoading(true)
    api.security.sessions()
      .then((r) => { setSessions(r.sessions); setCurrentId(r.current_session_id) })
      .catch(() => notify.error('Failed to load sessions'))
      .finally(() => setLoading(false))
  }, [])

  useEffect(() => { refresh() }, [refresh])

  const revoke = async (id: string) => {
    setRevoking(id)
    try {
      await api.security.revokeSession(id)
      notify.success('Session revoked')
      refresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to revoke session')
    } finally {
      setRevoking(null)
    }
  }

  const revokeAll = async () => {
    if (!confirm('Revoke all other active sessions? You will stay logged in on this device.')) return
    try {
      const r = await api.security.revokeOthers()
      notify.success(`Revoked ${r.revoked} session${r.revoked !== 1 ? 's' : ''}`)
      refresh()
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed')
    }
  }

  const others = sessions.filter((s) => s.id !== currentId)
  const byUser = sessions.reduce<Record<string, SessionInfo[]>>((acc, s) => {
    ;(acc[s.user_id] ??= []).push(s)
    return acc
  }, {})

  return (
    <div className="space-y-5">
      <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Security</h1>

      {/* Active sessions */}
      <div className="panel overflow-hidden">
        <div className="flex items-center justify-between px-4 py-3 border-b" style={{ borderColor: 'var(--border-subtle)' }}>
          <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>Active sessions</span>
          {others.length > 0 && (
            <Button size="sm" variant="ghost" onClick={revokeAll}>
              <LogOut size={12} className="mr-1" />
              Revoke all others
            </Button>
          )}
        </div>

        {loading ? (
          <p className="px-4 py-6 text-xs" style={{ color: 'var(--text-muted)' }}>Loading…</p>
        ) : sessions.length === 0 ? (
          <p className="px-4 py-6 text-xs" style={{ color: 'var(--text-muted)' }}>No active sessions found.</p>
        ) : (
          <div className="overflow-x-auto">
          <table className="w-full text-xs">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border-subtle)' }}>
                {(isAdmin
                  ? ['User', 'IP Address', 'Client', 'Created', 'Expires', '']
                  : ['IP Address', 'Client', 'Created', 'Expires', '']
                ).map((h) => (
                  <th key={h} className="px-4 py-2.5 text-left uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>{h}</th>
                ))}
              </tr>
            </thead>
            <tbody>
              {sessions.map((s) => {
                const isCurrent = s.id === currentId
                return (
                  <tr key={s.id} style={{ borderBottom: '1px solid var(--border-subtle)', background: isCurrent ? 'var(--accent-primary-subtle)' : undefined }}>
                    {isAdmin && (
                      <td className="px-4 py-3 font-mono text-xs" style={{ color: 'var(--text-muted)' }}>
                        {s.user_id.slice(0, 8)}
                      </td>
                    )}
                    <td className="px-4 py-3 font-mono" style={{ color: 'var(--text-secondary)' }}>
                      {s.ip_address ?? '—'}
                    </td>
                    <td className="px-4 py-3 flex items-center gap-2" style={{ color: 'var(--text-secondary)' }}>
                      <Monitor size={12} style={{ flexShrink: 0, color: 'var(--text-muted)' }} />
                      {parseUA(s.user_agent)}
                      {isCurrent && (
                        <span className="ml-1 px-1.5 py-0.5 rounded text-xs" style={{ background: 'var(--accent-primary)', color: '#fff' }}>
                          this session
                        </span>
                      )}
                    </td>
                    <td className="px-4 py-3" style={{ color: 'var(--text-muted)' }}>{formatDate(s.created_at)}</td>
                    <td className="px-4 py-3" style={{ color: 'var(--text-muted)' }}>{formatDate(s.expires_at)}</td>
                    <td className="px-4 py-3">
                      {!isCurrent && (
                        <button
                          onClick={() => revoke(s.id)}
                          disabled={revoking === s.id}
                          className="p-1 rounded transition-colors hover:opacity-80 disabled:opacity-40"
                          style={{ color: 'var(--accent-danger)' }}
                          title="Revoke session"
                        >
                          <Trash2 size={13} />
                        </button>
                      )}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
          </div>
        )}
      </div>

      {/* Admin: session summary by user */}
      {isAdmin && Object.keys(byUser).length > 1 && (
        <div className="panel p-4 space-y-2">
          <p className="text-xs font-medium mb-3" style={{ color: 'var(--text-secondary)' }}>Sessions by user</p>
          {Object.entries(byUser).map(([uid, list]) => (
            <div key={uid} className="flex items-center justify-between text-xs">
              <span className="font-mono" style={{ color: 'var(--text-muted)' }}>{uid.slice(0, 8)}…</span>
              <span style={{ color: 'var(--text-secondary)' }}>{list.length} active</span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
