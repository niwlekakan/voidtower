import { useState, useEffect, useCallback, useRef } from 'react'
import { Monitor, Trash2, LogOut, KeyRound, ShieldCheck, ShieldOff, Copy } from 'lucide-react'
import QRCode from 'react-qr-code'
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

// ── TOTP panel ────────────────────────────────────────────────────────────────

type TotpView = 'idle' | 'setup' | 'disable'

function TotpPanel() {
  const user = useAuthStore((s) => s.user)
  const setUser = useAuthStore((s) => s.setUser)
  const [view, setView] = useState<TotpView>('idle')
  const [secret, setSecret] = useState('')
  const [uri, setUri] = useState('')
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const codeRef = useRef<HTMLInputElement>(null)

  const startSetup = async () => {
    setBusy(true)
    try {
      const r = await api.totp.setup()
      setSecret(r.secret)
      setUri(r.uri)
      setView('setup')
      setCode('')
      setTimeout(() => codeRef.current?.focus(), 50)
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Failed to start TOTP setup')
    } finally {
      setBusy(false)
    }
  }

  const confirmEnable = async () => {
    if (code.length !== 6) return
    setBusy(true)
    try {
      await api.totp.enable(code)
      if (user) setUser({ ...user, totp_enabled: true })
      notify.success('Two-factor authentication enabled')
      setView('idle')
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Invalid code')
      setCode('')
      codeRef.current?.focus()
    } finally {
      setBusy(false)
    }
  }

  const confirmDisable = async () => {
    if (code.length !== 6) return
    setBusy(true)
    try {
      await api.totp.disable(code)
      if (user) setUser({ ...user, totp_enabled: false })
      notify.success('Two-factor authentication disabled')
      setView('idle')
    } catch (err) {
      notify.error(err instanceof ApiClientError ? err.message : 'Invalid code')
      setCode('')
      codeRef.current?.focus()
    } finally {
      setBusy(false)
    }
  }

  const inputStyle = {
    background: 'var(--bg-elevated)',
    border: '1px solid var(--border-default)',
    color: 'var(--text-primary)',
  }

  const enabled = user?.totp_enabled ?? false

  return (
    <div className="panel p-4 space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <KeyRound size={15} style={{ color: 'var(--accent-primary)' }} />
          <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
            Two-factor authentication
          </span>
          {enabled ? (
            <span className="flex items-center gap-1 px-1.5 py-0.5 rounded text-xs" style={{ background: 'var(--accent-success-subtle)', color: 'var(--accent-success)' }}>
              <ShieldCheck size={11} /> Enabled
            </span>
          ) : (
            <span className="flex items-center gap-1 px-1.5 py-0.5 rounded text-xs" style={{ background: 'var(--bg-elevated)', color: 'var(--text-muted)' }}>
              <ShieldOff size={11} /> Disabled
            </span>
          )}
        </div>
        {view === 'idle' && (
          enabled
            ? <Button size="sm" variant="ghost" onClick={() => { setView('disable'); setCode(''); setTimeout(() => codeRef.current?.focus(), 50) }}>Disable</Button>
            : <Button size="sm" variant="primary" onClick={startSetup} loading={busy}>Enable</Button>
        )}
      </div>

      {view === 'setup' && (
        <div className="space-y-3 pt-1">
          <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
            Scan the QR code with your authenticator app, then enter the 6-digit code to confirm.
          </p>

          <div className="flex justify-center p-3 rounded" style={{ background: '#fff' }}>
            <QRCode value={uri} size={180} />
          </div>

          <div className="flex items-center gap-2 px-3 py-2 rounded font-mono text-xs" style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>
            <span className="flex-1 break-all">{secret}</span>
            <button onClick={() => { navigator.clipboard.writeText(secret); notify.success('Secret copied') }} style={{ color: 'var(--text-muted)' }}>
              <Copy size={13} />
            </button>
          </div>
          <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
            Can't scan? Enter the secret manually in your app.
          </p>

          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Verification code</label>
            <input
              ref={codeRef}
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
              className="w-full px-3 py-2 rounded text-sm font-mono text-center tracking-widest outline-none"
              style={inputStyle}
              placeholder="000000"
              inputMode="numeric"
              maxLength={6}
            />
          </div>

          <div className="flex gap-2 justify-end">
            <Button size="sm" variant="ghost" onClick={() => setView('idle')}>Cancel</Button>
            <Button size="sm" variant="primary" onClick={confirmEnable} loading={busy} disabled={code.length !== 6}>
              Confirm &amp; enable
            </Button>
          </div>
        </div>
      )}

      {view === 'disable' && (
        <div className="space-y-3 pt-1">
          <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
            Enter your current authenticator code to disable two-factor authentication.
          </p>
          <input
            ref={codeRef}
            value={code}
            onChange={(e) => setCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
            className="w-full px-3 py-2 rounded text-sm font-mono text-center tracking-widest outline-none"
            style={inputStyle}
            placeholder="000000"
            inputMode="numeric"
            maxLength={6}
          />
          <div className="flex gap-2 justify-end">
            <Button size="sm" variant="ghost" onClick={() => setView('idle')}>Cancel</Button>
            <Button size="sm" variant="danger" onClick={confirmDisable} loading={busy} disabled={code.length !== 6}>
              Disable 2FA
            </Button>
          </div>
        </div>
      )}
    </div>
  )
}

// ── Page ──────────────────────────────────────────────────────────────────────

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

      <TotpPanel />

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
