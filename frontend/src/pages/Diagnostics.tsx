import { useEffect, useState } from 'react'
import { CheckCircle, XCircle, AlertTriangle, Info, RefreshCw, ChevronDown, ChevronRight } from 'lucide-react'
import { api } from '@/api/client'
import type { DiagCheck, DiagnosticsResponse } from '@/api/types'

const STATUS_CONFIG = {
  pass: { icon: CheckCircle, color: 'var(--accent-success)',  label: 'Pass' },
  warn: { icon: AlertTriangle, color: 'var(--accent-warning)', label: 'Warn' },
  fail: { icon: XCircle,      color: 'var(--accent-error)',   label: 'Fail' },
  info: { icon: Info,         color: 'var(--text-muted)',     label: 'Info' },
}

const CATEGORY_ORDER = ['Config', 'Database', 'System', 'Services', 'Containers', 'Backups', 'Networking', 'Network']

function CheckRow({ check }: { check: DiagCheck }) {
  const [open, setOpen] = useState(check.status === 'fail')
  const { icon: Icon, color } = STATUS_CONFIG[check.status]

  return (
    <div
      className="rounded border text-sm"
      style={{
        background: 'var(--bg-panel)',
        borderColor: check.status === 'fail'
          ? 'color-mix(in srgb, var(--accent-error) 35%, transparent)'
          : check.status === 'warn'
          ? 'color-mix(in srgb, var(--accent-warning) 25%, transparent)'
          : 'var(--border-subtle)',
      }}
    >
      <div
        className={`flex items-center gap-3 px-3 py-2.5 ${check.detail ? 'cursor-pointer' : ''}`}
        onClick={() => check.detail && setOpen(o => !o)}
      >
        <Icon size={15} style={{ color, flexShrink: 0 }} />
        <span className="flex-1 font-medium" style={{ color: 'var(--text-primary)' }}>{check.name}</span>
        <span className="flex-[2] text-xs" style={{ color: 'var(--text-secondary)' }}>{check.message}</span>
        {check.detail && (
          open
            ? <ChevronDown size={13} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
            : <ChevronRight size={13} style={{ color: 'var(--text-muted)', flexShrink: 0 }} />
        )}
      </div>

      {open && check.detail && (
        <div
          className="px-3 pb-2.5 pt-0 text-xs leading-relaxed"
          style={{
            color: 'var(--text-secondary)',
            borderTop: '1px solid var(--border-subtle)',
            paddingTop: '0.5rem',
          }}
        >
          {check.detail}
        </div>
      )}
    </div>
  )
}

function CategorySection({ name, checks }: { name: string; checks: DiagCheck[] }) {
  const hasFailure = checks.some(c => c.status === 'fail')
  const hasWarn    = checks.some(c => c.status === 'warn')
  const color = hasFailure ? 'var(--accent-error)' : hasWarn ? 'var(--accent-warning)' : 'var(--text-muted)'

  return (
    <div className="space-y-1.5">
      <h2 className="text-xs font-semibold uppercase tracking-wider px-0.5" style={{ color }}>
        {name}
      </h2>
      {checks.map(c => <CheckRow key={c.id} check={c} />)}
    </div>
  )
}

export default function DiagnosticsPage() {
  const [data, setData] = useState<DiagnosticsResponse | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const load = () => {
    setLoading(true)
    setError(null)
    api.diagnostics.run()
      .then(setData)
      .catch(e => setError(e.message ?? 'Failed to run diagnostics'))
      .finally(() => setLoading(false))
  }

  useEffect(() => { load() }, [])

  const grouped = data
    ? CATEGORY_ORDER.reduce<Record<string, DiagCheck[]>>((acc, cat) => {
        const checks = data.checks.filter(c => c.category === cat)
        if (checks.length) acc[cat] = checks
        return acc
      }, {})
    : {}

  const overall = data?.summary.overall
  const overallColor = overall === 'fail'
    ? 'var(--accent-error)'
    : overall === 'warn'
    ? 'var(--accent-warning)'
    : 'var(--accent-success)'

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Diagnostics</h1>
          {data && (
            <p className="text-xs mt-0.5" style={{ color: overallColor }}>
              Overall: {overall?.toUpperCase()} — {data.summary.pass} pass, {data.summary.warn} warn, {data.summary.fail} fail
            </p>
          )}
        </div>
        <button
          onClick={load}
          disabled={loading}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded text-sm transition-opacity hover:opacity-80 disabled:opacity-50"
          style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
        >
          <RefreshCw size={13} className={loading ? 'animate-spin' : ''} />
          Re-run
        </button>
      </div>

      {data && (
        <div className="flex gap-3 flex-wrap">
          {(
            [
              { label: 'Pass', value: data.summary.pass, color: 'var(--accent-success)' },
              { label: 'Warn', value: data.summary.warn, color: 'var(--accent-warning)' },
              { label: 'Fail', value: data.summary.fail, color: data.summary.fail > 0 ? 'var(--accent-error)' : 'var(--text-muted)' },
              { label: 'Info', value: data.summary.info, color: 'var(--text-muted)' },
            ] as const
          ).map(({ label, value, color }) => (
            <div
              key={label}
              className="flex flex-col items-center justify-center rounded px-4 py-2 min-w-[70px]"
              style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}
            >
              <span className="text-xl font-bold tabular-nums" style={{ color }}>{value}</span>
              <span className="text-xs uppercase tracking-wide" style={{ color: 'var(--text-muted)' }}>{label}</span>
            </div>
          ))}
        </div>
      )}

      {loading && !data && (
        <div className="flex items-center justify-center h-40 text-sm" style={{ color: 'var(--text-muted)' }}>
          Running checks…
        </div>
      )}

      {error && (
        <div
          className="rounded p-3 text-sm"
          style={{
            background: 'color-mix(in srgb, var(--accent-error) 10%, transparent)',
            color: 'var(--accent-error)',
            border: '1px solid color-mix(in srgb, var(--accent-error) 30%, transparent)',
          }}
        >
          {error}
        </div>
      )}

      {Object.entries(grouped).map(([cat, checks]) => (
        <CategorySection key={cat} name={cat} checks={checks} />
      ))}
    </div>
  )
}
