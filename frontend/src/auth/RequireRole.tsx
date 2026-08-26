import type { Role } from '@/api/types'
import { useAuthStore } from '@/store/auth'
import { roleAllowed } from './roles'

export default function RequireRole({ allowed, children }: { allowed: readonly Role[]; children: React.ReactNode }) {
  const role = useAuthStore((state) => state.user?.role)
  if (roleAllowed(role, allowed)) return <>{children}</>
  return (
    <div className="mx-auto max-w-xl rounded-lg p-8 text-center" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
      <div className="text-xs font-semibold uppercase tracking-[0.14em]" style={{ color: 'var(--accent-danger)' }}>Access restricted</div>
      <h1 className="mt-2 text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Your role cannot open this workflow</h1>
      <p className="mt-2 text-sm" style={{ color: 'var(--text-secondary)' }}>The server enforces the same role boundary on every request.</p>
    </div>
  )
}
