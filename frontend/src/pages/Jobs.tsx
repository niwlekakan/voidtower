import { useMemo, useState } from 'react'
import { ChevronLeft, RefreshCw, Search, XCircle } from 'lucide-react'
import { Link, useParams } from 'react-router-dom'
import { api, ApiClientError } from '@/api/client'
import type { DurableJobState } from '@/api/types'
import { useJobDetail, useJobList } from '@/hooks/useOperationRecords'
import { notify } from '@/store/notifications'
import {
  formatOperationTime,
  JobDetailView,
  JobProgress,
  JobStateBadge,
  ReadStateNotice,
} from '@/components/operations/OperationViews'

const STATES: Array<{ value: 'all' | DurableJobState; label: string }> = [
  { value: 'all', label: 'All states' },
  { value: 'awaiting_approval', label: 'Awaiting approval' },
  { value: 'queued', label: 'Queued' },
  { value: 'running', label: 'Running' },
  { value: 'needs_attention', label: 'Needs attention' },
  { value: 'succeeded', label: 'Succeeded' },
  { value: 'failed', label: 'Failed' },
  { value: 'cancelled', label: 'Cancelled' },
  { value: 'rejected', label: 'Rejected' },
  { value: 'expired', label: 'Expired' },
]

function ActionButton({ onClick, disabled, children, danger = false }: { onClick: () => void; disabled?: boolean; children: React.ReactNode; danger?: boolean }) {
  return (
    <button onClick={onClick} disabled={disabled} className="inline-flex items-center gap-1.5 rounded px-3 py-1.5 text-xs font-medium transition-opacity hover:opacity-80 disabled:opacity-40" style={{ background: danger ? 'color-mix(in srgb, var(--accent-danger) 14%, transparent)' : 'var(--bg-panel)', border: `1px solid ${danger ? 'color-mix(in srgb, var(--accent-danger) 45%, transparent)' : 'var(--border-subtle)'}`, color: danger ? 'var(--accent-danger)' : 'var(--text-secondary)' }}>
      {children}
    </button>
  )
}

function JobDetailPage({ id }: { id: string }) {
  const jobState = useJobDetail(id)
  const [cancelling, setCancelling] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)
  const job = jobState.data

  const cancel = async () => {
    if (!job || !window.confirm(`Cancel job ${job.id.slice(0, 8)}? The worker will stop only at a safe checkpoint.`)) return
    setCancelling(true)
    setActionError(null)
    try {
      const response = await api.operationJobs.cancel(job.id)
      jobState.accept(response.job)
      notify.info('Cancellation requested', `Job ${job.id.slice(0, 8)} was confirmed from the durable queue.`)
    } catch (error) {
      await jobState.refresh()
      const message = error instanceof ApiClientError ? error.message : 'The cancellation result was ambiguous.'
      setActionError(`${message} The job was refetched before this status was shown.`)
    } finally {
      setCancelling(false)
    }
  }

  return (
    <div className="mx-auto max-w-5xl space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <Link to="/jobs" className="inline-flex items-center gap-1.5 text-xs hover:opacity-80" style={{ color: 'var(--text-secondary)' }}><ChevronLeft size={14} /> All jobs</Link>
        <div className="flex gap-2">
          <ActionButton onClick={() => void jobState.refresh()} disabled={jobState.refreshing}><RefreshCw size={13} className={jobState.refreshing ? 'animate-spin' : ''} /> Refresh</ActionButton>
          {job && (job.state === 'queued' || job.state === 'running') && <ActionButton onClick={() => void cancel()} disabled={cancelling} danger><XCircle size={13} /> {cancelling ? 'Checking state…' : 'Cancel job'}</ActionButton>}
        </div>
      </div>
      <ReadStateNotice stale={jobState.stale} error={actionError || jobState.error} deadlineReached={jobState.deadlineReached} />
      {jobState.loading && !job && <div className="py-16 text-center text-sm" style={{ color: 'var(--text-muted)' }}>Loading durable job…</div>}
      {!jobState.loading && !job && <div className="rounded-lg p-10 text-center" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-muted)' }}>No confirmed job record is available.</div>}
      {job && <JobDetailView job={job} />}
    </div>
  )
}

function JobListPage() {
  const jobsState = useJobList(50)
  const [state, setState] = useState<'all' | DurableJobState>('all')
  const [query, setQuery] = useState('')
  const jobs = useMemo(() => jobsState.data ?? [], [jobsState.data])
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase()
    return jobs.filter(job => (state === 'all' || job.state === state) && (!needle || `${job.action} ${job.resource.display_name} ${job.resource.kind} ${job.id}`.toLowerCase().includes(needle)))
  }, [jobs, query, state])

  return (
    <div className="space-y-4">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-[0.16em]" style={{ color: 'var(--accent-primary)' }}>Durable operations</div>
          <h1 className="mt-1 text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>Jobs</h1>
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Newest 50 records · server state is authoritative</p>
        </div>
        <ActionButton onClick={() => void jobsState.refresh()} disabled={jobsState.refreshing}><RefreshCw size={13} className={jobsState.refreshing ? 'animate-spin' : ''} /> Refresh</ActionButton>
      </header>

      <ReadStateNotice stale={jobsState.stale} error={jobsState.error} deadlineReached={jobsState.deadlineReached} />

      <div className="grid gap-2 sm:grid-cols-[1fr_13rem]">
        <label className="relative">
          <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2" style={{ color: 'var(--text-muted)' }} />
          <input value={query} onChange={event => setQuery(event.target.value)} placeholder="Find action, resource, or job ID" className="w-full rounded-md py-2 pl-9 pr-3 text-sm outline-none" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} />
        </label>
        <select value={state} onChange={event => setState(event.target.value as 'all' | DurableJobState)} className="rounded-md px-3 py-2 text-sm outline-none" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }}>
          {STATES.map(option => <option key={option.value} value={option.value}>{option.label}</option>)}
        </select>
      </div>

      <div className="overflow-hidden rounded-lg" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        {jobsState.loading && jobs.length === 0 && <div className="py-16 text-center text-sm" style={{ color: 'var(--text-muted)' }}>Loading durable jobs…</div>}
        {!jobsState.loading && filtered.length === 0 && <div className="py-16 text-center text-sm" style={{ color: 'var(--text-muted)' }}>{jobs.length === 0 ? 'No durable jobs have been recorded yet.' : 'No jobs match these filters.'}</div>}
        {filtered.map(job => (
          <Link key={job.id} to={`/jobs/${encodeURIComponent(job.id)}`} className="group block border-b p-4 last:border-b-0 hover:bg-[var(--bg-elevated)]" style={{ borderColor: 'var(--border-subtle)' }}>
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2"><span className="font-mono text-[10px]" style={{ color: 'var(--text-muted)' }}>{job.id.slice(0, 8)}</span><JobStateBadge state={job.state} /></div>
                <div className="mt-2 text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{job.action}</div>
                <div className="mt-0.5 truncate text-xs" style={{ color: 'var(--text-secondary)' }}>{job.resource.display_name} · {job.resource.kind} · {job.actor.actor_type}/{job.ingress}</div>
              </div>
              <time className="text-[11px]" style={{ color: 'var(--text-muted)' }}>{formatOperationTime(job.submitted_at)}</time>
            </div>
            <div className="mt-3"><JobProgress job={job} /></div>
          </Link>
        ))}
      </div>
    </div>
  )
}

export default function JobsPage() {
  const { id } = useParams<{ id: string }>()
  return id ? <JobDetailPage id={id} /> : <JobListPage />
}
