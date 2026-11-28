import { useState } from 'react'
import { Check, ChevronLeft, RefreshCw, ShieldAlert, X } from 'lucide-react'
import { Link, useParams } from 'react-router-dom'
import { api, ApiClientError } from '@/api/client'
import type { DurableApprovalStatus } from '@/api/types'
import { useApprovalDetail, useApprovalList, useJobDetail } from '@/hooks/useOperationRecords'
import { notify } from '@/store/notifications'
import {
  ApprovalDetailView,
  ApprovalStateBadge,
  formatOperationTime,
  JobDetailView,
  ReadStateNotice,
} from '@/components/operations/OperationViews'

const FILTERS: Array<{ value: 'all' | DurableApprovalStatus; label: string }> = [
  { value: 'pending', label: 'Pending' },
  { value: 'approved', label: 'Approved' },
  { value: 'rejected', label: 'Rejected' },
  { value: 'expired', label: 'Expired' },
  { value: 'stale', label: 'Stale' },
  { value: 'all', label: 'All' },
]

function ApprovalDetailPage({ id }: { id: string }) {
  const approvalState = useApprovalDetail(id)
  const approval = approvalState.data
  const jobState = useJobDetail(approval?.job_id)
  const [comment, setComment] = useState('')
  const [deciding, setDeciding] = useState<'approve' | 'reject' | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)

  const decide = async (decision: 'approve' | 'reject') => {
    if (!approval || deciding) return
    const verb = decision === 'approve' ? 'Approve' : 'Reject'
    if (!window.confirm(`${verb} approval ${approval.id.slice(0, 8)} for job ${approval.job_id.slice(0, 8)}?`)) return
    setDeciding(decision)
    setActionError(null)
    try {
      const response = decision === 'approve'
        ? await api.approvals.approve(approval.id, comment)
        : await api.approvals.reject(approval.id, comment)
      jobState.accept(response.job)
      await Promise.all([approvalState.refresh(), jobState.refresh()])
      notify.success(`Approval ${decision === 'approve' ? 'approved' : 'rejected'}`, `Record ${approval.id.slice(0, 8)} was decided.`)
    } catch (error) {
      await Promise.all([approvalState.refresh(), jobState.refresh()])
      const message = error instanceof ApiClientError ? error.message : 'The decision result was ambiguous.'
      setActionError(`${message} The approval and job were refetched before this status was shown.`)
    } finally {
      setDeciding(null)
    }
  }

  return (
    <div className="mx-auto max-w-5xl space-y-4">
      <div className="flex items-center justify-between gap-3">
        <Link to="/approvals" className="inline-flex items-center gap-1.5 text-xs hover:opacity-80" style={{ color: 'var(--text-secondary)' }}><ChevronLeft size={14} /> All approvals</Link>
        <button onClick={() => void Promise.all([approvalState.refresh(), jobState.refresh()])} disabled={approvalState.refreshing || jobState.refreshing} className="inline-flex items-center gap-1.5 rounded px-3 py-1.5 text-xs" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}><RefreshCw size={13} className={approvalState.refreshing || jobState.refreshing ? 'animate-spin' : ''} /> Refresh</button>
      </div>
      <ReadStateNotice stale={approvalState.stale || jobState.stale} error={actionError || approvalState.error || jobState.error} deadlineReached={approvalState.deadlineReached || jobState.deadlineReached} />
      {approvalState.loading && !approval && <div className="py-16 text-center text-sm" style={{ color: 'var(--text-muted)' }}>Loading approval…</div>}
      {!approvalState.loading && !approval && <div className="rounded-lg p-10 text-center" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-muted)' }}>No confirmed approval record is available.</div>}
      {approval && <ApprovalDetailView approval={approval} />}
      {approval?.status === 'pending' && (
        <section className="rounded-lg p-4" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
          <div className="flex items-center gap-2"><ShieldAlert size={15} style={{ color: 'var(--accent-warning, #f59e0b)' }} /><h2 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Decide this exact approval</h2></div>
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Review the immutable job plan below. Decisions are final for this approval record.</p>
          <textarea value={comment} maxLength={500} onChange={event => setComment(event.target.value)} placeholder="Optional decision comment" rows={3} className="mt-3 w-full resize-y rounded-md p-3 text-sm outline-none" style={{ background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)' }} />
          <div className="mt-1 text-right font-mono text-[10px]" style={{ color: 'var(--text-muted)' }}>{comment.length}/500</div>
          <div className="mt-3 flex flex-wrap justify-end gap-2">
            <button onClick={() => void decide('reject')} disabled={deciding !== null} className="inline-flex items-center gap-1.5 rounded px-3 py-2 text-xs font-semibold disabled:opacity-40" style={{ color: 'var(--accent-danger)', border: '1px solid color-mix(in srgb, var(--accent-danger) 45%, transparent)', background: 'color-mix(in srgb, var(--accent-danger) 10%, transparent)' }}><X size={14} /> {deciding === 'reject' ? 'Rejecting…' : 'Reject approval'}</button>
            <button onClick={() => void decide('approve')} disabled={deciding !== null} className="inline-flex items-center gap-1.5 rounded px-3 py-2 text-xs font-semibold disabled:opacity-40" style={{ color: 'var(--accent-success)', border: '1px solid color-mix(in srgb, var(--accent-success) 45%, transparent)', background: 'color-mix(in srgb, var(--accent-success) 10%, transparent)' }}><Check size={14} /> {deciding === 'approve' ? 'Approving…' : 'Approve exact plan'}</button>
          </div>
        </section>
      )}
      {jobState.data && <JobDetailView job={jobState.data} />}
    </div>
  )
}

function ApprovalListPage() {
  const [filter, setFilter] = useState<'all' | DurableApprovalStatus>('pending')
  const state = useApprovalList(filter === 'all' ? undefined : filter, 50)
  const approvals = state.data ?? []
  return (
    <div className="space-y-4">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <div className="text-[10px] font-semibold uppercase tracking-[0.16em]" style={{ color: 'var(--accent-warning, #f59e0b)' }}>Human decision queue</div>
          <h1 className="mt-1 text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>Approvals</h1>
          <p className="mt-1 text-xs" style={{ color: 'var(--text-muted)' }}>Each decision is bound to one immutable job plan</p>
        </div>
        <button onClick={() => void state.refresh()} disabled={state.refreshing} className="inline-flex items-center gap-1.5 rounded px-3 py-1.5 text-xs" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}><RefreshCw size={13} className={state.refreshing ? 'animate-spin' : ''} /> Refresh</button>
      </header>
      <ReadStateNotice stale={state.stale} error={state.error} deadlineReached={state.deadlineReached} />
      <div className="flex flex-wrap gap-1.5">
        {FILTERS.map(option => <button key={option.value} onClick={() => setFilter(option.value)} className="rounded-full px-3 py-1 text-xs" style={{ background: filter === option.value ? 'color-mix(in srgb, var(--accent-primary) 15%, transparent)' : 'var(--bg-panel)', border: `1px solid ${filter === option.value ? 'var(--accent-primary)' : 'var(--border-subtle)'}`, color: filter === option.value ? 'var(--accent-primary)' : 'var(--text-muted)' }}>{option.label}</button>)}
      </div>
      <div className="overflow-hidden rounded-lg" style={{ background: 'var(--bg-panel)', border: '1px solid var(--border-subtle)' }}>
        {state.loading && approvals.length === 0 && <div className="py-16 text-center text-sm" style={{ color: 'var(--text-muted)' }}>Loading approvals…</div>}
        {!state.loading && approvals.length === 0 && <div className="py-16 text-center text-sm" style={{ color: 'var(--text-muted)' }}>{filter === 'pending' ? 'No approvals are waiting for a decision.' : 'No approvals match this status.'}</div>}
        {approvals.map(approval => (
          <Link key={approval.id} to={`/approvals/${encodeURIComponent(approval.id)}`} className="block border-b p-4 last:border-b-0 hover:bg-[var(--bg-elevated)]" style={{ borderColor: 'var(--border-subtle)' }}>
            <div className="flex flex-wrap items-start justify-between gap-3">
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2"><span className="font-mono text-[10px]" style={{ color: 'var(--text-muted)' }}>{approval.id.slice(0, 8)}</span><ApprovalStateBadge status={approval.status} /></div>
                <div className="mt-2 text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{approval.requirement}</div>
                <div className="mt-0.5 text-xs" style={{ color: 'var(--text-secondary)' }}>{approval.reason}</div>
                <div className="mt-1 font-mono text-[10px]" style={{ color: 'var(--text-muted)' }}>job {approval.job_id.slice(0, 8)}</div>
              </div>
              <time className="text-[11px]" style={{ color: 'var(--text-muted)' }}>{approval.status === 'pending' ? `Expires ${formatOperationTime(approval.expires_at)}` : formatOperationTime(approval.decided_at)}</time>
            </div>
          </Link>
        ))}
      </div>
    </div>
  )
}

export default function ApprovalsPage() {
  const { id } = useParams<{ id: string }>()
  return id ? <ApprovalDetailPage id={id} /> : <ApprovalListPage />
}
