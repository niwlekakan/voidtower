import { useState } from 'react'
import { Check, ChevronLeft, RefreshCw, X } from 'lucide-react'
import { useLocation, useNavigate } from 'react-router-dom'
import { api, ApiClientError } from '@/api/client'
import { ADMIN_ROLES, roleAllowed } from '@/auth/roles'
import { useAuthStore } from '@/store/auth'
import { useApprovalDetail, useApprovalList, useJobDetail } from '@/hooks/useOperationRecords'
import { approvalStateTone } from '@/operations/state'
import { ApprovalDetailView, ApprovalStateBadge, JobDetailView, operationToneColor, ReadStateNotice } from '@/components/operations/OperationViews'
import NativePanelShell, { EmptyState, IconBtn, LoadingState, NativeRow, StatusDot } from './NativePanelShell'

function selectedApproval(pathname: string): string | null {
  const match = pathname.match(/^\/approvals\/([^/]+)$/)
  return match ? decodeURIComponent(match[1]) : null
}

function ApprovalsPanelContent() {
  const location = useLocation()
  const navigate = useNavigate()
  const selected = selectedApproval(location.pathname)
  const [filter, setFilter] = useState<'pending' | 'all'>('pending')
  const list = useApprovalList(filter === 'pending' ? 'pending' : undefined, 50)
  const approvalState = useApprovalDetail(selected ?? undefined)
  const approval = approvalState.data
  const jobState = useJobDetail(approval?.job_id)
  const [comment, setComment] = useState('')
  const [deciding, setDeciding] = useState<'approve' | 'reject' | null>(null)
  const [actionError, setActionError] = useState<string | null>(null)

  if (selected) {
    const decide = async (decision: 'approve' | 'reject') => {
      if (!approval || deciding || !window.confirm(`${decision === 'approve' ? 'Approve' : 'Reject'} approval ${approval.id.slice(0, 8)}?`)) return
      setDeciding(decision)
      setActionError(null)
      try {
        const response = decision === 'approve'
          ? await api.approvals.approve(approval.id, comment)
          : await api.approvals.reject(approval.id, comment)
        jobState.accept(response.job)
        await Promise.all([approvalState.refresh(), jobState.refresh()])
      } catch (error) {
        await Promise.all([approvalState.refresh(), jobState.refresh()])
        setActionError(error instanceof ApiClientError ? error.message : 'Decision was ambiguous; both records were refetched.')
      } finally {
        setDeciding(null)
      }
    }
    const actions = approval?.status === 'pending' ? (
      <>
        <button onClick={() => void decide('reject')} disabled={deciding !== null} style={{ padding: '5px 8px', borderRadius: 5, border: '1px solid color-mix(in srgb, var(--accent-danger) 45%, transparent)', background: 'transparent', color: 'var(--accent-danger)', fontSize: 10 }}><X size={11} style={{ display: 'inline', marginRight: 3 }} />Reject</button>
        <button onClick={() => void decide('approve')} disabled={deciding !== null} style={{ padding: '5px 8px', borderRadius: 5, border: '1px solid color-mix(in srgb, var(--accent-success) 45%, transparent)', background: 'color-mix(in srgb, var(--accent-success) 10%, transparent)', color: 'var(--accent-success)', fontSize: 10 }}><Check size={11} style={{ display: 'inline', marginRight: 3 }} />Approve exact plan</button>
      </>
    ) : undefined
    return (
      <NativePanelShell actions={actions}>
        <div style={{ display: 'flex', justifyContent: 'space-between', padding: '6px 8px', borderBottom: '1px solid var(--border-subtle)' }}>
          <IconBtn title="Back to approvals" onClick={() => navigate('/approvals')}><ChevronLeft size={14} /></IconBtn>
          <IconBtn title="Refresh records" onClick={() => void Promise.all([approvalState.refresh(), jobState.refresh()])}><RefreshCw size={13} /></IconBtn>
        </div>
        <div style={{ padding: 8, display: 'grid', gap: 8 }}>
          <ReadStateNotice stale={approvalState.stale || jobState.stale} error={actionError || approvalState.error || jobState.error} deadlineReached={approvalState.deadlineReached || jobState.deadlineReached} />
          {approvalState.loading && !approval ? <LoadingState /> : approval ? <ApprovalDetailView approval={approval} /> : <EmptyState text="Approval not found" />}
          {approval?.status === 'pending' && <textarea value={comment} maxLength={500} onChange={event => setComment(event.target.value)} placeholder="Optional decision comment" rows={2} style={{ width: '100%', resize: 'vertical', padding: 7, borderRadius: 5, background: 'var(--bg-surface)', border: '1px solid var(--border-subtle)', color: 'var(--text-primary)', fontSize: 11 }} />}
          {jobState.data && <JobDetailView job={jobState.data} compact />}
        </div>
      </NativePanelShell>
    )
  }

  const approvals = list.data ?? []
  return (
    <NativePanelShell tabs={[{ id: 'pending', label: 'Pending' }, { id: 'all', label: 'History' }]} activeTab={filter} onTabChange={id => setFilter(id as 'pending' | 'all')} actions={<IconBtn title="Refresh approvals" onClick={() => void list.refresh()} disabled={list.refreshing}><RefreshCw size={13} /></IconBtn>}>
      <div style={{ padding: '4px 8px' }}><ReadStateNotice stale={list.stale} error={list.error} deadlineReached={list.deadlineReached} /></div>
      {list.loading && approvals.length === 0 ? <LoadingState /> : approvals.length === 0 ? <EmptyState text="No approvals are waiting" /> : approvals.map(item => (
        <NativeRow key={item.id} onClick={() => navigate(`/approvals/${encodeURIComponent(item.id)}`)} style={{ cursor: 'pointer', alignItems: 'flex-start' }}>
          <StatusDot color={operationToneColor(approvalStateTone(item.status))} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}><span style={{ fontSize: 10, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{item.requirement}</span><ApprovalStateBadge status={item.status} /></div>
            <div style={{ marginTop: 2, fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{item.reason} · job {item.job_id.slice(0, 8)}</div>
          </div>
        </NativeRow>
      ))}
    </NativePanelShell>
  )
}

export default function NativeApprovalsPanel() {
  const role = useAuthStore(state => state.user?.role)
  if (!roleAllowed(role, ADMIN_ROLES)) return <NativePanelShell><EmptyState text="Your role cannot open approvals" /></NativePanelShell>
  return <ApprovalsPanelContent />
}
