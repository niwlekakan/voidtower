import { useState, type FormEvent } from 'react'
import { useNavigate } from 'react-router-dom'
import { Shield } from 'lucide-react'
import { api } from '@/api/client'
import { useAuthStore } from '@/store/auth'
import { ApiClientError } from '@/api/client'
import Button from '@/components/ui/Button'

export default function LoginPage() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)
  const setUser = useAuthStore((s) => s.setUser)
  const navigate = useNavigate()

  const submit = async (e: FormEvent) => {
    e.preventDefault()
    setError('')
    setLoading(true)
    try {
      const { user } = await api.auth.login(username, password)
      setUser(user)
      navigate('/dashboard')
    } catch (err) {
      setError(err instanceof ApiClientError ? err.message : 'Login failed')
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="h-full flex items-center justify-center" style={{ background: 'var(--bg-root)' }}>
      <div className="card w-full max-w-sm">
        <div className="flex items-center gap-2 mb-6">
          <Shield size={22} style={{ color: 'var(--accent-primary)' }} />
          <span className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>VoidTower</span>
        </div>
        <form onSubmit={submit} className="space-y-4">
          <div>
            <label className="block text-xs mb-1.5" style={{ color: 'var(--text-secondary)' }}>Username</label>
            <input
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="w-full px-3 py-2 rounded text-sm outline-none focus-visible:ring-0"
              style={{
                background: 'var(--bg-elevated)',
                border: '1px solid var(--border-default)',
                color: 'var(--text-primary)',
              }}
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
              className="w-full px-3 py-2 rounded text-sm outline-none"
              style={{
                background: 'var(--bg-elevated)',
                border: '1px solid var(--border-default)',
                color: 'var(--text-primary)',
              }}
              autoComplete="current-password"
              required
            />
          </div>
          {error && <p className="text-xs" style={{ color: 'var(--accent-danger)' }}>{error}</p>}
          <Button variant="primary" className="w-full justify-center" loading={loading} type="submit">
            Sign in
          </Button>
        </form>
        <p className="mt-4 text-xs text-center">
          <a href="/bootstrap" style={{ color: 'var(--accent-primary)' }}>First-time setup</a>
        </p>
      </div>
    </div>
  )
}
