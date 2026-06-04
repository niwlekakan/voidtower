import { useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { Shield, RefreshCw } from 'lucide-react'
import { api, ApiClientError } from '@/api/client'
import { useAuthStore } from '@/store/auth'
import Button from '@/components/ui/Button'

const CODENAMES = [
  'phantom', 'specter', 'cipher', 'vector', 'wraith', 'vortex', 'oracle',
  'bastion', 'citadel', 'raven', 'sentinel', 'nexus', 'axiom', 'herald',
  'forge', 'sentry', 'shade', 'grimoire', 'remnant', 'warden', 'signal',
  'vertex', 'corona', 'eclipse', 'fractal', 'kestrel', 'mirage', 'nether',
  'pulse', 'quasar', 'stasis', 'torrent', 'umbra', 'zenith', 'abyss',
  'catalyst', 'daemon', 'entropy', 'flux', 'glitch',
]

export default function BootstrapPage() {
  const [token, setToken] = useState('')
  const [username, setUsername] = useState('')
  const [codeIdx, setCodeIdx] = useState(0)
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const setUser = useAuthStore((s) => s.setUser)
  const navigate = useNavigate()

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    if (password !== confirm) { setError('Passwords do not match'); return }
    if (password.length < 8) { setError('Password must be at least 8 characters'); return }
    setError('')
    setLoading(true)
    try {
      const { user } = await api.auth.bootstrap(token, username, password)
      setUser(user)
      navigate('/dashboard')
    } catch (err) {
      setError(err instanceof ApiClientError ? err.message : 'Bootstrap failed')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="h-full flex items-center justify-center" style={{ background: 'var(--bg-root)' }}>
      <div className="card w-full max-w-sm">
        <div className="flex items-center gap-2 mb-2">
          <Shield size={22} style={{ color: 'var(--accent-primary)' }} />
          <span className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Initial Setup</span>
        </div>
        <p className="text-xs mb-5" style={{ color: 'var(--text-muted)' }}>
          Enter the bootstrap token shown during install, then create your owner account.
        </p>
        <form onSubmit={submit} className="space-y-4">
          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Bootstrap Token</label>
            <input type="text" value={token} onChange={(e) => setToken(e.target.value)}
              className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
              style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
              autoComplete="off" required />
          </div>
          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Username</label>
            <div className="flex gap-2">
              <input type="text" value={username} onChange={(e) => setUsername(e.target.value)}
                className="flex-1 px-3 py-2 rounded text-sm outline-none font-mono"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
                autoComplete="username" required />
              <button type="button"
                onClick={() => { const n = (codeIdx + 1) % CODENAMES.length; setCodeIdx(n); setUsername(CODENAMES[n]) }}
                className="flex items-center gap-1.5 px-3 py-2 rounded text-xs transition-colors hover:opacity-80"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-subtle)', color: 'var(--text-muted)' }}
                title="Suggest a codename">
                <RefreshCw size={11} /> Suggest
              </button>
            </div>
          </div>
          {[
            { label: 'Password',        value: password, set: setPassword, auto: 'new-password' },
            { label: 'Confirm Password', value: confirm,  set: setConfirm,  auto: 'new-password' },
          ].map(({ label, value, set, auto }) => (
            <div key={label}>
              <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>{label}</label>
              <input type="password" value={value} onChange={(e) => set(e.target.value)}
                className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
                style={{ background: 'var(--bg-elevated)', border: '1px solid var(--border-default)', color: 'var(--text-primary)' }}
                autoComplete={auto} required />
            </div>
          ))}
          {error && <p className="text-xs" style={{ color: 'var(--accent-danger)' }}>{error}</p>}
          <Button variant="primary" className="w-full justify-center" loading={loading} type="submit">
            Create owner account
          </Button>
        </form>
        <p className="mt-4 text-xs text-center">
          <a href="/login" style={{ color: 'var(--accent-primary)' }}>Already have an account?</a>
        </p>
      </div>
    </div>
  )
}
