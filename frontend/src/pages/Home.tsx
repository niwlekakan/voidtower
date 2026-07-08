import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { api } from '@/api/client'
import type { Alert } from '@/api/types'
import { useAuthStore } from '@/store/auth'

function greeting() {
  const hour = new Date().getHours()
  if (hour < 12) return 'Good morning'
  if (hour < 18) return 'Good afternoon'
  return 'Good evening'
}

function today() {
  return new Date().toLocaleDateString(undefined, { weekday: 'long', month: 'long', day: 'numeric' })
}

function severityColor(severity: string): string {
  switch (severity) {
    case 'critical': return 'var(--accent-danger)'
    case 'warning':  return 'var(--accent-warning)'
    default:         return 'var(--accent-secondary)'
  }
}

/**
 * Landing page for two audiences that both want a plain-language summary
 * instead of the technical dashboard: the desktop app (see App.tsx's
 * isTauri() branch on the index route) and any `member`-role user (who
 * never sees the technical dashboard at all, on any client — see
 * App.tsx's MemberGate and Sidebar.tsx's member-only nav). The plain
 * browser for every other role keeps redirecting `/` to `/dashboard`
 * exactly as before.
 */
export default function HomePage() {
  const user = useAuthStore((s) => s.user)
  const navigate = useNavigate()
  const isMember = user?.role === 'member'
  const [alerts, setAlerts] = useState<Alert[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (isMember) { setLoading(false); return }
    api.alerts.list('active')
      .then((data) => setAlerts(data.alerts))
      .catch(() => { /* Home is best-effort; the dashboard shows real errors */ })
      .finally(() => setLoading(false))
  }, [isMember])

  const worstSeverity = alerts.reduce<'info' | 'warning' | 'critical' | null>((worst, a) => {
    const rank = { info: 0, warning: 1, critical: 2 }
    if (!worst || rank[a.severity] > rank[worst]) return a.severity
    return worst
  }, null)

  return (
    <div className="p-8 max-w-2xl mx-auto flex flex-col gap-4">
      <div>
        <h1 className="text-2xl font-bold" style={{ color: 'var(--text-primary)' }}>
          {greeting()}{user ? `, ${user.username}` : ''}
        </h1>
        <p className="text-sm mt-1" style={{ color: 'var(--text-secondary)' }}>{today()}</p>
      </div>

      {isMember ? (
        <button
          onClick={() => navigate('/apps')}
          className="flex items-center gap-3 rounded-2xl p-4 text-left transition-colors"
          style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}
        >
          <span className="w-3 h-3 rounded-full shrink-0" style={{ background: 'var(--accent-primary)' }} />
          <span className="flex flex-col">
            <span className="font-semibold" style={{ color: 'var(--text-primary)' }}>My apps</span>
            <span className="text-xs mt-0.5" style={{ color: 'var(--text-secondary)' }}>
              Open, deploy, and manage the apps you've been given access to
            </span>
          </span>
        </button>
      ) : !loading && (
        <button
          onClick={() => navigate('/dashboard')}
          className="flex items-center gap-3 rounded-2xl p-4 text-left transition-colors"
          style={{ background: 'var(--bg-card)', border: '1px solid var(--border-default)' }}
        >
          <span
            className="w-3 h-3 rounded-full shrink-0"
            style={{ background: worstSeverity ? severityColor(worstSeverity) : 'var(--accent-success)' }}
          />
          <span className="flex flex-col">
            <span className="font-semibold" style={{ color: 'var(--text-primary)' }}>
              {alerts.length === 0
                ? 'Everything looks good'
                : `${alerts.length} thing${alerts.length === 1 ? '' : 's'} need attention`}
            </span>
            <span className="text-xs mt-0.5" style={{ color: 'var(--text-secondary)' }}>
              Tap for the technical view
            </span>
          </span>
        </button>
      )}
    </div>
  )
}
