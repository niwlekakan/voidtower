import { Link } from 'react-router-dom'

export default function NotFoundPage() {
  return (
    <div className="flex flex-col items-center justify-center h-64 gap-3">
      <div className="text-4xl font-mono font-bold" style={{ color: 'var(--text-disabled)' }}>404</div>
      <div className="text-sm" style={{ color: 'var(--text-muted)' }}>Page not found</div>
      <Link to="/dashboard" className="text-sm" style={{ color: 'var(--accent-primary)' }}>Go to dashboard</Link>
    </div>
  )
}
