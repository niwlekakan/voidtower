import { useState, useRef, useEffect, type FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { Shield, KeyRound } from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import { useAuthStore } from '@/store/auth'
import Button from '@/components/ui/Button'

interface PublicSettings {
  instance_name: string
  login_tagline: string
  login_bg_url: string
  instance_logo: string
}

export default function LoginPage() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [totpCode, setTotpCode] = useState('')
  const [step, setStep] = useState<'credentials' | 'totp'>('credentials')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [pub, setPub] = useState<PublicSettings | null>(null)
  const [oidcEnabled, setOidcEnabled] = useState(false)
  const [oidcLabel, setOidcLabel] = useState('Login with Authentik')
  const setUser = useAuthStore((s) => s.setUser)
  const navigate = useNavigate()
  const totpRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (step === 'totp') totpRef.current?.focus()
  }, [step])

  useEffect(() => {
    fetch('/api/settings/public')
      .then(r => r.ok ? r.json() : null)
      .then((d: PublicSettings | null) => { if (d) setPub(d) })
      .catch(() => {})
  }, [])

  useEffect(() => {
    api.auth.oidcStatus()
      .then((s) => { setOidcEnabled(s.enabled); setOidcLabel(s.button_label) })
      .catch(() => {})
  }, [])

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setError('')
    setLoading(true)
    try {
      const code = step === 'totp' ? totpCode : undefined
      const { user } = await api.auth.login(username, password, code)
      setUser(user)
      navigate('/dashboard')
    } catch (err) {
      if (err instanceof ApiClientError && err.code === 'totp_required') {
        setStep('totp')
        setTotpCode('')
      } else {
        const msg = err instanceof ApiClientError ? err.message : 'Login failed'
        if (err instanceof ApiClientError && err.status === 429) {
          setError('Too many failed attempts. Try again later.')
        } else {
          setError(msg)
        }
        if (step === 'totp') setTotpCode('')
      }
    } finally {
      setLoading(false)
    }
  }

  const inputCls = 'w-full px-3 py-2 rounded text-sm outline-none'
  const inputStyle = {
    background: 'var(--bg-elevated)',
    border: '1px solid var(--border-default)',
    color: 'var(--text-primary)',
  }

  const bgStyle = pub?.login_bg_url
    ? { backgroundImage: `url(${pub.login_bg_url})`, backgroundSize: 'cover', backgroundPosition: 'center' }
    : {}

  return (
    <div className="h-full flex items-center justify-center" style={{ background: 'var(--bg-root)', ...bgStyle }}>
      <div className="card w-full max-w-sm">
        <div className="flex items-center gap-2 mb-6">
          {pub?.instance_logo
            ? <img src={pub.instance_logo} alt="logo" style={{ width: 28, height: 28, objectFit: 'contain', flexShrink: 0 }} />
            : <Shield size={22} style={{ color: 'var(--accent-primary)' }} />
          }
          <span className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            {pub?.instance_name ?? 'VoidTower'}
          </span>
        </div>
        {pub?.login_tagline && (
          <p className="text-xs mb-4" style={{ color: 'var(--text-muted)' }}>{pub.login_tagline}</p>
        )}

        <form onSubmit={submit} className="space-y-4">
          {step === 'credentials' ? (
            <>
              <div>
                <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Username</label>
                <input
                  value={username}
                  onChange={(e) => setUsername(e.target.value)}
                  className={inputCls}
                  style={inputStyle}
                  autoFocus
                  autoComplete="username"
                  required
                />
              </div>
              <div>
                <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Password</label>
                <input
                  type="password"
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  className={inputCls}
                  style={inputStyle}
                  autoComplete="current-password"
                  required
                />
              </div>
            </>
          ) : (
            <div>
              <div className="flex items-center gap-2 mb-3">
                <KeyRound size={15} style={{ color: 'var(--accent-primary)' }} />
                <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                  Two-factor authentication
                </span>
              </div>
              <p className="text-xs mb-3" style={{ color: 'var(--text-secondary)' }}>
                Enter the 6-digit code from your authenticator app.
              </p>
              <input
                ref={totpRef}
                value={totpCode}
                onChange={(e) => setTotpCode(e.target.value.replace(/\D/g, '').slice(0, 6))}
                className={`${inputCls} text-center tracking-widest font-mono text-base`}
                style={inputStyle}
                placeholder="000000"
                inputMode="numeric"
                autoComplete="one-time-code"
                maxLength={6}
                required
              />
              <button
                type="button"
                className="mt-2 text-xs"
                style={{ color: 'var(--text-muted)' }}
                onClick={() => { setStep('credentials'); setError(''); setTotpCode('') }}
              >
                ← Back
              </button>
            </div>
          )}

          {error && (
            <p className="text-xs" style={{ color: 'var(--accent-danger)' }}>{error}</p>
          )}

          <Button variant="primary" className="w-full justify-center" loading={loading} type="submit">
            {step === 'totp' ? 'Verify' : 'Sign in'}
          </Button>
        </form>

        {step === 'credentials' && oidcEnabled && (
          <div className="mt-4">
            <div className="flex items-center gap-2 mb-3">
              <div className="flex-1 h-px" style={{ background: 'var(--border-default)' }} />
              <span className="text-xs" style={{ color: 'var(--text-muted)' }}>or</span>
              <div className="flex-1 h-px" style={{ background: 'var(--border-default)' }} />
            </div>
            <a href="/api/auth/oidc/login">
              <Button variant="secondary" className="w-full justify-center">
                <Shield size={14} />
                {oidcLabel}
              </Button>
            </a>
          </div>
        )}

        {step === 'credentials' && (
          <p className="mt-4 text-xs text-center">
            <a href="/bootstrap" style={{ color: 'var(--accent-primary)' }}>First-time setup</a>
          </p>
        )}
      </div>
    </div>
  )
}
