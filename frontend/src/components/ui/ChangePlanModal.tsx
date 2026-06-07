import { X, AlertTriangle, Info } from 'lucide-react'

export interface ChangePlanChange {
  label: string
  value: string
}

export interface ChangePlan {
  title: string
  risk: 'low' | 'medium' | 'high'
  changes: ChangePlanChange[]
  preview: string | null
}

interface Props {
  plan: ChangePlan
  onConfirm: () => void
  onCancel: () => void
  confirming?: boolean
}

const RISK_COLOR: Record<string, string> = {
  low: 'var(--accent-success)',
  medium: 'var(--accent-warning)',
  high: 'var(--accent-error)',
}

export default function ChangePlanModal({ plan, onConfirm, onCancel, confirming }: Props) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60">
      <div
        className="w-full max-w-lg rounded-lg flex flex-col gap-4 p-5 shadow-2xl"
        style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', maxHeight: '85vh' }}
      >
        {/* Header */}
        <div className="flex items-center justify-between gap-2">
          <div className="flex items-center gap-2">
            {plan.risk === 'high' ? (
              <AlertTriangle size={16} style={{ color: RISK_COLOR[plan.risk] }} />
            ) : (
              <Info size={16} style={{ color: RISK_COLOR[plan.risk] }} />
            )}
            <h2 className="font-semibold text-sm" style={{ color: 'var(--text-primary)' }}>
              Change Plan — {plan.title}
            </h2>
          </div>
          <div className="flex items-center gap-2">
            <span
              className="text-xs px-2 py-0.5 rounded-full font-medium"
              style={{ background: `${RISK_COLOR[plan.risk]}20`, color: RISK_COLOR[plan.risk] }}
            >
              {plan.risk} risk
            </span>
            <button onClick={onCancel}>
              <X size={15} style={{ color: 'var(--text-muted)' }} />
            </button>
          </div>
        </div>

        {/* Change table */}
        <div className="rounded overflow-hidden" style={{ border: '1px solid var(--border-subtle)' }}>
          <table className="w-full text-sm">
            <tbody>
              {plan.changes.map((c, i) => (
                <tr key={i} style={{ borderBottom: i < plan.changes.length - 1 ? '1px solid var(--border-subtle)' : undefined }}>
                  <td className="px-3 py-2 font-medium w-36 shrink-0" style={{ color: 'var(--text-muted)', background: 'var(--bg-surface)' }}>
                    {c.label}
                  </td>
                  <td className="px-3 py-2 font-mono break-all" style={{ color: 'var(--text-primary)' }}>
                    {c.value}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        {/* Optional config preview */}
        {plan.preview && (
          <div className="flex flex-col gap-1 overflow-hidden">
            <span className="text-xs" style={{ color: 'var(--text-muted)' }}>Config preview</span>
            <pre
              className="text-xs p-3 rounded overflow-auto"
              style={{
                background: 'var(--bg-surface)',
                border: '1px solid var(--border-subtle)',
                color: 'var(--text-secondary)',
                maxHeight: '180px',
                fontFamily: 'var(--font-mono, monospace)',
              }}
            >
              {plan.preview}
            </pre>
          </div>
        )}

        {/* Actions */}
        <div className="flex gap-2 justify-end pt-1">
          <button
            onClick={onCancel}
            className="px-3 py-1.5 rounded text-sm"
            style={{ background: 'var(--bg-surface)', color: 'var(--text-secondary)', border: '1px solid var(--border-subtle)' }}
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            disabled={confirming}
            className="px-3 py-1.5 rounded text-sm font-medium disabled:opacity-50"
            style={{ background: RISK_COLOR[plan.risk], color: '#fff' }}
          >
            {confirming ? 'Executing…' : 'Confirm & Execute'}
          </button>
        </div>
      </div>
    </div>
  )
}
