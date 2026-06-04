import { useEffect, useState } from 'react'
import { CheckCircle, XCircle, RefreshCw, ChevronDown, ChevronRight } from 'lucide-react'
import { api } from '@/api/client'
import type { Capability } from '@/api/types'

const CATEGORY_ORDER = [
  'Containers',
  'Services',
  'Virtualisation',
  'Storage',
  'Backups',
  'Networking',
  'GPU',
  'Package Manager',
]

function CapabilityCard({ cap }: { cap: Capability }) {
  const [expanded, setExpanded] = useState(false)

  return (
    <div
      className="rounded border p-3 text-sm"
      style={{
        background: 'var(--bg-panel)',
        borderColor: cap.detected ? 'var(--border-subtle)' : 'color-mix(in srgb, var(--accent-error) 30%, transparent)',
      }}
    >
      <div className="flex items-start gap-3">
        {cap.detected
          ? <CheckCircle size={16} style={{ color: 'var(--accent-success)', flexShrink: 0, marginTop: 1 }} />
          : <XCircle    size={16} style={{ color: 'var(--accent-error)',   flexShrink: 0, marginTop: 1 }} />
        }

        <div className="flex-1 min-w-0">
          <div className="flex items-baseline gap-2 flex-wrap">
            <span className="font-medium" style={{ color: 'var(--text-primary)' }}>
              {cap.name}
            </span>
            {cap.version && (
              <span className="text-xs font-mono" style={{ color: 'var(--text-muted)' }}>
                {cap.version}
              </span>
            )}
            {!cap.detected && (
              <span
                className="text-xs px-1.5 py-0.5 rounded"
                style={{ background: 'color-mix(in srgb, var(--accent-error) 15%, transparent)', color: 'var(--accent-error)' }}
              >
                not found
              </span>
            )}
          </div>

          <p className="mt-0.5 text-xs leading-relaxed" style={{ color: 'var(--text-secondary)' }}>
            {cap.description}
          </p>

          {!cap.detected && (
            <div className="mt-2">
              <button
                onClick={() => setExpanded(e => !e)}
                className="flex items-center gap-1 text-xs hover:opacity-80 transition-opacity"
                style={{ color: 'var(--accent-primary)' }}
              >
                {expanded ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
                How to enable
              </button>

              {expanded && (
                <div
                  className="mt-1.5 rounded p-2.5 text-xs space-y-1"
                  style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)' }}
                >
                  <div>
                    <span style={{ color: 'var(--text-muted)' }}>Requires: </span>
                    <code style={{ color: 'var(--accent-secondary)' }}>{cap.required_dep}</code>
                  </div>
                  <div>
                    <span style={{ color: 'var(--text-muted)' }}>Install: </span>
                    <code className="break-all" style={{ color: 'var(--text-secondary)' }}>{cap.how_to_enable}</code>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

function CategorySection({ name, caps }: { name: string; caps: Capability[] }) {
  const detected = caps.filter(c => c.detected).length
  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <h2 className="text-xs font-semibold uppercase tracking-wider" style={{ color: 'var(--text-muted)' }}>
          {name}
        </h2>
        <span className="text-xs" style={{ color: 'var(--text-muted)' }}>
          {detected}/{caps.length}
        </span>
      </div>
      <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
        {caps.map(cap => (
          <CapabilityCard key={cap.id} cap={cap} />
        ))}
      </div>
    </div>
  )
}

export default function CapabilitiesPage() {
  const [data, setData] = useState<{ capabilities: Capability[]; summary: { total: number; detected: number; missing: number } } | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const load = () => {
    setLoading(true)
    setError(null)
    api.capabilities.list()
      .then(setData)
      .catch(e => setError(e.message ?? 'Failed to load capabilities'))
      .finally(() => setLoading(false))
  }

  useEffect(() => { load() }, [])

  const grouped = data
    ? CATEGORY_ORDER.reduce<Record<string, Capability[]>>((acc, cat) => {
        const caps = data.capabilities.filter(c => c.category === cat)
        if (caps.length) acc[cat] = caps
        return acc
      }, {})
    : {}

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Capabilities</h1>
          {data && (
            <p className="text-xs mt-0.5" style={{ color: 'var(--text-muted)' }}>
              {data.summary.detected} of {data.summary.total} detected
              {data.summary.missing > 0 && ` · ${data.summary.missing} missing`}
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
          Refresh
        </button>
      </div>

      {data && (
        <div className="flex gap-3 flex-wrap">
          {[
            { label: 'Detected',    value: data.summary.detected, color: 'var(--accent-success)' },
            { label: 'Missing',     value: data.summary.missing,  color: data.summary.missing > 0 ? 'var(--accent-warning)' : 'var(--text-muted)' },
            { label: 'Total',       value: data.summary.total,    color: 'var(--text-secondary)' },
          ].map(({ label, value, color }) => (
            <div
              key={label}
              className="flex flex-col items-center justify-center rounded px-4 py-2 min-w-[80px]"
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
          Scanning system…
        </div>
      )}

      {error && (
        <div className="rounded p-3 text-sm" style={{ background: 'color-mix(in srgb, var(--accent-error) 10%, transparent)', color: 'var(--accent-error)', border: '1px solid color-mix(in srgb, var(--accent-error) 30%, transparent)' }}>
          {error}
        </div>
      )}

      {Object.entries(grouped).map(([cat, caps]) => (
        <CategorySection key={cat} name={cat} caps={caps} />
      ))}
    </div>
  )
}
