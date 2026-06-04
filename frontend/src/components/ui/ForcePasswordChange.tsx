import { useState } from 'react'
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

export default function ForcePasswordChange() {
  const { user, setUser } = useAuthStore()
  const [username, setUsername] = useState(user?.username ?? '')
  const [password, setPassword] = useState('')
  const [confirm, setConfirm] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const [codeIdx, setCodeIdx] = useState(() => Math.floor(Math.random() * CODENAMES.length))

  const suggestName = () => {
    const next = (codeIdx + 1) % CODENAMES.length
    setCodeIdx(next)
    setUsername(CODENAMES[next])
  }

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (password !== confirm) { setError('Passwords do not match'); return }
    if (password.length < 8) { setError('Password must be at least 8 characters'); return }
    setError('')
    setLoading(true)
    try {
      const newName = username !== user?.username ? username : undefined
      await api.users.changePassword(password, newName)
      const { user: updated } = await api.auth.me()
      setUser(updated)
    } catch (err) {
      setError(err instanceof ApiClientError ? err.message : 'Failed to update credentials')
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
            Set your credentials
          </span>
        </div>
        <p className="text-xs mb-5" style={{ color: 'var(--text-muted)' }}>
          This account requires a password change before you can continue.
          Pick a codename or keep your existing username.
        </p>
        <form onSubmit={submit} className="space-y-4">
          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>
              Username
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="flex-1 px-3 py-2 rounded text-sm outline-none font-mono"
                style={{
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border-default)',
                  color: 'var(--text-primary)',
                }}
                autoComplete="username"
                required
              />
              <button
                type="button"
                onClick={suggestName}
                className="flex items-center gap-1.5 px-3 py-2 rounded text-xs transition-colors hover:opacity-80"
                style={{
                  background: 'var(--bg-elevated)',
                  border: '1px solid var(--border-subtle)',
                  color: 'var(--text-muted)',
                }}
                title="Suggest a codename"
              >
                <RefreshCw size={11} />
                Suggest
              </button>
            </div>
          </div>
          {(['New Password', 'Confirm Password'] as const).map((label) => {
            const isConfirm = label === 'Confirm Password'
            return (
              <div key={label}>
                <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>
                  {label}
                </label>
                <input
                  type="password"
                  value={isConfirm ? confirm : password}
                  onChange={(e) => isConfirm ? setConfirm(e.target.value) : setPassword(e.target.value)}
                  className="w-full px-3 py-2 rounded text-sm outline-none font-mono"
                  style={{
                    background: 'var(--bg-elevated)',
                    border: '1px solid var(--border-default)',
                    color: 'var(--text-primary)',
                  }}
                  autoComplete="new-password"
                  required
                />
              </div>
            )
          })}
          {error && (
            <p className="text-xs" style={{ color: 'var(--accent-danger)' }}>{error}</p>
          )}
          <Button variant="primary" className="w-full justify-center" loading={loading} type="submit">
            Save &amp; continue
          </Button>
        </form>
      </div>
    </div>
  )
}
