import type {
  DurableApproval,
  DurableApprovalStatus,
  DurableJob,
  DurableJobState,
  DurableOperationPlan,
} from '@/api/types'
import {
  approvalStateLabel,
  approvalStateTone,
  jobStateLabel,
  jobStateTone,
  type OperationTone,
} from '@/operations/state'

const TONE_COLORS: Record<OperationTone, string> = {
  info: 'var(--accent-primary)',
  success: 'var(--accent-success)',
  warning: 'var(--accent-warning, #f59e0b)',
  error: 'var(--accent-danger)',
  muted: 'var(--text-muted)',
}

export function operationToneColor(tone: OperationTone): string {
  return TONE_COLORS[tone]
}

export function JobStateBadge({ state }: { state: DurableJobState }) {
  const color = operationToneColor(jobStateTone(state))
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-semibold" style={{ color, background: `color-mix(in srgb, ${color} 14%, transparent)`, border: `1px solid color-mix(in srgb, ${color} 35%, transparent)` }}>
      <span className="h-1.5 w-1.5 rounded-full" style={{ background: color }} />
      {jobStateLabel(state)}
    </span>
  )
}

export function ApprovalStateBadge({ status }: { status: DurableApprovalStatus }) {
  const color = operationToneColor(approvalStateTone(status))
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-semibold" style={{ color, background: `color-mix(in srgb, ${color} 14%, transparent)`, border: `1px solid color-mix(in srgb, ${color} 35%, transparent)` }}>
      <span className="h-1.5 w-1.5 rounded-full" style={{ background: color }} />
      {approvalStateLabel(status)}
    </span>
  )
}

export function formatOperationTime(timestamp: number | null): string {
  if (timestamp === null) return '—'
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'medium' }).format(new Date(timestamp * 1000))
}

export function JobProgress({ job }: { job: DurableJob }) {
  const total = Math.max(job.progress_total, 0)
  const current = Math.min(Math.max(job.progress_current, 0), total || 0)
  const percent = total > 0 ? Math.round((current / total) * 100) : 0
  const color = operationToneColor(jobStateTone(job.state))
  return (
    <div className="space-y-1.5">
      <div className="flex items-center justify-between gap-3 text-[11px]" style={{ color: 'var(--text-muted)' }}>
        <span>{job.progress_message || (total > 0 ? `${current} of ${total} steps` : 'No progress reported')}</span>
        {total > 0 && <span className="font-mono">{percent}%</span>}
      </div>
      <div className="h-1.5 overflow-hidden rounded-full" style={{ background: 'var(--bg-elevated)' }}>
        <div className="h-full rounded-full transition-[width] duration-300" style={{ width: `${percent}%`, background: color }} />
      </div>
    </div>
  )
}

const MAX_DEPTH = 3
const MAX_KEYS = 16
const MAX_ITEMS = 20
const MAX_STRING = 500
const SECRET_KEY = /(secret|password|passwd|token|authorization|credential|private[_-]?key|api[_-]?key|cookie)/i

function boundedString(value: string): string {
  return value.length > MAX_STRING ? `${value.slice(0, MAX_STRING)}… [truncated]` : value
}

function ValueNode({ value, depth, field }: { value: unknown; depth: number; field?: string }) {
  if (field && SECRET_KEY.test(field)) return <span>[redacted]</span>
  if (value === null) return <span style={{ color: 'var(--text-muted)' }}>null</span>
  if (typeof value === 'string') return <span className="whitespace-pre-wrap break-words">{boundedString(value)}</span>
  if (typeof value === 'number' || typeof value === 'boolean') return <span>{String(value)}</span>
  if (depth >= MAX_DEPTH) return <span style={{ color: 'var(--text-muted)' }}>[nested value truncated]</span>
  if (Array.isArray(value)) {
    const shown = value.slice(0, MAX_ITEMS)
    return (
      <ol className="space-y-1 pl-4">
        {shown.map((item, index) => <li key={index}><ValueNode value={item} depth={depth + 1} /></li>)}
        {value.length > shown.length && <li style={{ color: 'var(--text-muted)' }}>… {value.length - shown.length} more items</li>}
      </ol>
    )
  }
  if (typeof value === 'object') {
    const entries = Object.entries(value as Record<string, unknown>)
    const shown = entries.slice(0, MAX_KEYS)
    return (
      <dl className="grid grid-cols-[minmax(7rem,auto)_1fr] gap-x-3 gap-y-1">
        {shown.map(([key, item]) => (
          <div key={key} className="contents">
            <dt className="font-mono" style={{ color: 'var(--text-muted)' }}>{boundedString(key)}</dt>
            <dd className="min-w-0"><ValueNode value={item} depth={depth + 1} field={key} /></dd>
          </div>
        ))}
        {entries.length > shown.length && <div className="col-span-2" style={{ color: 'var(--text-muted)' }}>… {entries.length - shown.length} more fields</div>}
      </dl>
    )
  }
  return <span style={{ color: 'var(--text-muted)' }}>[unsupported value]</span>
}

export function BoundedValue({ value }: { value: unknown }) {
  return (
    <div className="overflow-auto rounded-md p-3 text-xs leading-relaxed" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>
      <ValueNode value={value} depth={0} />
    </div>
  )
}

export function OperationPlanView({ plan }: { plan: DurableOperationPlan }) {
  return (
    <section className="space-y-3">
      <div className="flex flex-wrap items-start justify-between gap-2">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-[0.14em]" style={{ color: 'var(--text-muted)' }}>Immutable plan</div>
          <h3 className="mt-1 text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{plan.title}</h3>
        </div>
        <span className="rounded px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide" style={{ background: 'var(--bg-elevated)', color: 'var(--text-secondary)' }}>{plan.risk}</span>
      </div>
      {plan.changes.length > 0 && (
        <dl className="grid gap-2 sm:grid-cols-2">
          {plan.changes.map((change, index) => (
            <div key={`${change.label}-${index}`} className="rounded-md p-2.5" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)' }}>
              <dt className="text-[10px] uppercase tracking-wide" style={{ color: 'var(--text-muted)' }}>{change.label}</dt>
              <dd className="mt-1 break-words text-xs" style={{ color: 'var(--text-primary)' }}>{boundedString(change.value)}</dd>
            </div>
          ))}
        </dl>
      )}
      {plan.preview && <div className="max-h-48 overflow-auto whitespace-pre-wrap rounded-md p-3 font-mono text-[11px]" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}>{boundedString(plan.preview)}</div>}
      {plan.steps.length > 0 && (
        <div className="space-y-0">
          {plan.steps.map((step, index) => (
            <div key={`${step.kind}-${index}`} className="relative flex gap-3 pb-3 last:pb-0">
              <div className="flex w-5 flex-col items-center">
                <span className="mt-1 h-2 w-2 rounded-full" style={{ background: 'var(--accent-primary)' }} />
                {index < plan.steps.length - 1 && <span className="mt-1 w-px flex-1" style={{ background: 'var(--border-subtle)' }} />}
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>{step.name}</div>
                <div className="mt-0.5 font-mono text-[10px]" style={{ color: 'var(--text-muted)' }}>{step.kind} · {step.retry_class} · {step.recovery_class}</div>
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  )
}

export function JobDetailView({ job, compact = false }: { job: DurableJob; compact?: boolean }) {
  const rail = operationToneColor(jobStateTone(job.state))
  return (
    <div className={compact ? 'space-y-4' : 'space-y-5'}>
      <section className="relative overflow-hidden rounded-lg p-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        <span className="absolute inset-y-0 left-0 w-1" style={{ background: rail }} />
        <div className="flex flex-wrap items-start justify-between gap-3 pl-1">
          <div className="min-w-0">
            <div className="font-mono text-[10px] uppercase tracking-[0.12em]" style={{ color: 'var(--text-muted)' }}>{job.id}</div>
            <h2 className="mt-1 break-words text-base font-semibold" style={{ color: 'var(--text-primary)' }}>{job.action}</h2>
            <p className="mt-1 text-xs" style={{ color: 'var(--text-secondary)' }}>{job.resource.display_name} · {job.resource.kind} · revision {job.resource.revision}</p>
          </div>
          <JobStateBadge state={job.state} />
        </div>
        <div className="mt-4 pl-1"><JobProgress job={job} /></div>
      </section>

      <section className="grid gap-3 rounded-lg p-4 sm:grid-cols-2" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        <Meta label="Actor" value={`${job.actor.actor_type}${job.actor.id ? ` · ${job.actor.id}` : ''}`} />
        <Meta label="Ingress" value={job.ingress} />
        <Meta label="Submitted" value={formatOperationTime(job.submitted_at)} />
        <Meta label="Updated" value={formatOperationTime(job.updated_at)} />
        <Meta label="Started" value={formatOperationTime(job.started_at)} />
        <Meta label="Finished" value={formatOperationTime(job.finished_at)} />
      </section>

      <section className="rounded-lg p-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        <OperationPlanView plan={job.plan} />
      </section>

      {job.error && (
        <section className="rounded-lg p-4" style={{ background: 'color-mix(in srgb, var(--accent-danger) 8%, var(--bg-panel))', border: '1px solid color-mix(in srgb, var(--accent-danger) 35%, var(--border-subtle))' }}>
          <div className="text-[10px] font-semibold uppercase tracking-[0.14em]" style={{ color: 'var(--accent-danger)' }}>Operation error · {job.error.code}</div>
          <p className="mt-2 text-sm" style={{ color: 'var(--text-primary)' }}>{job.error.message}</p>
          <p className="mt-1 text-[11px]" style={{ color: 'var(--text-muted)' }}>{job.error.retryable ? 'The provider classified this error as retryable. No retry is automatic.' : 'The provider did not classify this error as retryable.'}</p>
        </section>
      )}

      {job.state === 'needs_attention' && (
        <section className="rounded-lg p-4 text-sm" style={{ background: 'color-mix(in srgb, var(--accent-warning, #f59e0b) 9%, var(--bg-panel))', border: '1px solid color-mix(in srgb, var(--accent-warning, #f59e0b) 38%, var(--border-subtle))', color: 'var(--text-primary)' }}>
          Provider state is uncertain. Inspect the target before deciding what to do next; VoidTower will not retry or resubmit this operation automatically.
        </section>
      )}

      {job.result !== null && (
        <section className="space-y-2 rounded-lg p-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
          <div className="text-[10px] font-semibold uppercase tracking-[0.14em]" style={{ color: 'var(--text-muted)' }}>Bounded result</div>
          <BoundedValue value={job.result} />
        </section>
      )}
    </div>
  )
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <dt className="text-[10px] uppercase tracking-wide" style={{ color: 'var(--text-muted)' }}>{label}</dt>
      <dd className="mt-0.5 break-words text-xs" style={{ color: 'var(--text-primary)' }}>{value}</dd>
    </div>
  )
}

export function ApprovalDetailView({ approval }: { approval: DurableApproval }) {
  const rail = operationToneColor(approvalStateTone(approval.status))
  return (
    <section className="relative overflow-hidden rounded-lg p-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
      <span className="absolute inset-y-0 left-0 w-1" style={{ background: rail }} />
      <div className="flex flex-wrap items-start justify-between gap-3 pl-1">
        <div>
          <div className="font-mono text-[10px] uppercase tracking-[0.12em]" style={{ color: 'var(--text-muted)' }}>{approval.id}</div>
          <h2 className="mt-1 text-base font-semibold" style={{ color: 'var(--text-primary)' }}>{approval.requirement}</h2>
          <p className="mt-1 text-sm" style={{ color: 'var(--text-secondary)' }}>{approval.reason}</p>
        </div>
        <ApprovalStateBadge status={approval.status} />
      </div>
      <dl className="mt-4 grid gap-3 pl-1 sm:grid-cols-2">
        <Meta label="Job" value={approval.job_id} />
        <Meta label="Requested" value={formatOperationTime(approval.requested_at)} />
        <Meta label="Expires" value={formatOperationTime(approval.expires_at)} />
        <Meta label="Decided" value={formatOperationTime(approval.decided_at)} />
        {approval.decided_by && <Meta label="Decided by" value={approval.decided_by} />}
        {approval.decision_comment && <Meta label="Comment" value={approval.decision_comment} />}
      </dl>
    </section>
  )
}

export function ReadStateNotice({ stale, error, deadlineReached }: { stale: boolean; error: string | null; deadlineReached: boolean }) {
  if (!stale && !error && !deadlineReached) return null
  return (
    <div className="rounded-md px-3 py-2 text-xs" style={{ background: 'color-mix(in srgb, var(--accent-warning, #f59e0b) 9%, var(--bg-panel))', border: '1px solid color-mix(in srgb, var(--accent-warning, #f59e0b) 35%, var(--border-subtle))', color: 'var(--text-secondary)' }}>
      {stale && 'Showing the last confirmed record. '}
      {error && `${error} `}
      {deadlineReached && 'Automatic foreground refresh stopped; the durable operation continues on the server.'}
    </div>
  )
}
