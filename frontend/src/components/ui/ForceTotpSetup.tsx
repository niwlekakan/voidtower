import { useEffect, useRef, useState } from 'react'
import { Shield, Copy } from 'lucide-react'
import QRCode from 'react-qr-code'
import { api, ApiClientError } from '@/api/client'
import { useAuthStore } from '@/store/auth'
import Button from '@/components/ui/Button'

export default function ForceTotpSetup() {
  const setUser = useAuthStore((s) => s.setUser)
  const [secret, setSecret] = useState('')
  const [uri, setUri] = useState('')
  const [code, setCode] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(true)
  const codeRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    api.totp.setup()
      .then((r) => { setSecret(r.secret); setUri(r.uri) })
      .catch(() => setError('Failed to start two-factor setup'))
      .finally(() => setLoading(false))
  }, [])

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (code.length !== 6) return
    setError('')
    setLoading(true)
    try {
      await api.totp.enable(code)
      const { user: updated } = await api.auth.me()
      setUser(updated)
    } catch (err) {
      setError(err instanceof ApiClientError ? err.message : 'Invalid code')
      setCode('')
      codeRef.current?.focus()
      setLoading(false)
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ background: 'rgba(0,0,0,0.88)', backdropFilter: 'blur(6px)' }}
    >
      <div className="card w-full max-w-sm">
        <div className="flex items-center gap-2 mb-1">
          <Shield size={20} style={{ color: 'var(--accent-primary)' }} />
          <span className="font-semibold" style={{ color: 'var(--text-primary)' }}>
            Set up two-factor authentication
          </span>
        </div>
        <p className="text-xs mb-5" style={{ color: 'var(--text-muted)' }}>
          This account requires two-factor authentication before you can continue.
          Scan the QR code with your authenticator app, then enter the 6-digit code.
        </p>
        <form onSubmit={submit} className="space-y-4">
          <div className="flex justify-center p-3 rounded" style={{ background: '#fff' }}>
            {uri && <QRCode value={uri} size={180} />}
          </div>

          <div className="flex items-center gap-2 px-3 py-2 rounded font-mono text-xs" style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}>
            <span className="flex-1 break-all">{secret}</span>
            <button
              type="button"
              onClick={() => navigator.clipboard.writeText(secret)}
              style={{ color: 'var(--text-muted)' }}
            >
              <Copy size={13} />
            </button>
          </div>
          <p className="text-xs" style={{ color: 'var(--text-muted)' }}>
            Can't scan? Enter the secret manually in your app.
          </p>

          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>
              Verification code
            </label>
            <input
              ref={codeRef}
              value={code}
              onChange={(e) => setCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
              className="w-full px-3 py-2 rounded text-sm font-mono text-center tracking-widest outline-none"
              style={{
                background: 'var(--bg-elevated)',
                border: '1px solid var(--border-default)',
                color: 'var(--text-primary)',
              }}
              placeholder="000000"
              inputMode="numeric"
              maxLength={6}
              autoFocus
            />
          </div>
          {error && (
            <p className="text-xs" style={{ color: 'var(--accent-danger)' }}>{error}</p>
          )}
          <Button variant="primary" className="w-full justify-center" loading={loading} type="submit" disabled={code.length !== 6}>
            Confirm &amp; continue
          </Button>
        </form>
      </div>
    </div>
  )
}
