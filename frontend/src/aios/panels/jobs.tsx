import { useMemo, useState } from 'react'
import { ChevronLeft, RefreshCw, XCircle } from 'lucide-react'
import { useLocation, useNavigate } from 'react-router-dom'
import { api, ApiClientError } from '@/api/client'
import { OPERATOR_ROLES, roleAllowed } from '@/auth/roles'
import { useAuthStore } from '@/store/auth'
import { useJobDetail, useJobList } from '@/hooks/useOperationRecords'
import { jobStateTone } from '@/operations/state'
import { JobDetailView, JobStateBadge, operationToneColor, ReadStateNotice } from '@/components/operations/OperationViews'
import NativePanelShell, { EmptyState, IconBtn, LoadingState, NativeRow, StatusDot } from './NativePanelShell'

function selectedJob(pathname: string): string | null {
  const match = pathname.match(/^\/jobs\/([^/]+)$/)
  return match ? decodeURIComponent(match[1]) : null
}

function JobsPanelContent() {
  const location = useLocation()
  const navigate = useNavigate()
  const selected = selectedJob(location.pathname)
  const list = useJobList(50)
  const detail = useJobDetail(selected ?? undefined)
  const [search, setSearch] = useState('')
  const [cancelling, setCancelling] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)
  const jobs = useMemo(() => {
    const needle = search.trim().toLowerCase()
    return (list.data ?? []).filter(job => !needle || `${job.action} ${job.resource.display_name} ${job.id}`.toLowerCase().includes(needle))
  }, [list.data, search])

  if (selected) {
    const job = detail.data
    const cancel = async () => {
      if (!job || !window.confirm(`Cancel job ${job.id.slice(0, 8)}?`)) return
      setCancelling(true)
      setActionError(null)
      try {
        const response = await api.operationJobs.cancel(job.id)
        detail.accept(response.job)
      } catch (error) {
        await detail.refresh()
        setActionError(error instanceof ApiClientError ? error.message : 'Cancellation was ambiguous; the job was refetched.')
      } finally {
        setCancelling(false)
      }
    }
    return (
      <NativePanelShell actions={job && (job.state === 'queued' || job.state === 'running') ? <button onClick={() => void cancel()} disabled={cancelling} style={{ padding: '5px 9px', borderRadius: 5, border: '1px solid color-mix(in srgb, var(--accent-danger) 45%, transparent)', background: 'color-mix(in srgb, var(--accent-danger) 10%, transparent)', color: 'var(--accent-danger)', fontSize: 10 }}><XCircle size={11} style={{ display: 'inline', marginRight: 4 }} />{cancelling ? 'Checking…' : 'Cancel job'}</button> : undefined}>
        <div style={{ display: 'flex', justifyContent: 'space-between', padding: '6px 8px', borderBottom: '1px solid var(--border-subtle)' }}>
          <IconBtn title="Back to jobs" onClick={() => navigate('/jobs')}><ChevronLeft size={14} /></IconBtn>
          <IconBtn title="Refresh job" onClick={() => void detail.refresh()} disabled={detail.refreshing}><RefreshCw size={13} /></IconBtn>
        </div>
        <div style={{ padding: 8 }}>
          <ReadStateNotice stale={detail.stale} error={actionError || detail.error} deadlineReached={detail.deadlineReached} />
          {detail.loading && !job ? <LoadingState /> : job ? <JobDetailView job={job} compact /> : <EmptyState text="Job not found" />}
        </div>
      </NativePanelShell>
    )
  }

  return (
    <NativePanelShell search={search} onSearch={setSearch} searchPlaceholder="Find jobs…" actions={<IconBtn title="Refresh jobs" onClick={() => void list.refresh()} disabled={list.refreshing}><RefreshCw size={13} /></IconBtn>}>
      <div style={{ padding: '4px 8px' }}><ReadStateNotice stale={list.stale} error={list.error} deadlineReached={list.deadlineReached} /></div>
      {list.loading && jobs.length === 0 ? <LoadingState /> : jobs.length === 0 ? <EmptyState text="No durable jobs" /> : jobs.map(job => (
        <NativeRow key={job.id} onClick={() => navigate(`/jobs/${encodeURIComponent(job.id)}`)} style={{ cursor: 'pointer', alignItems: 'flex-start' }}>
          <StatusDot color={operationToneColor(jobStateTone(job.state))} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 5 }}><span style={{ fontSize: 10, color: 'var(--text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{job.action}</span><JobStateBadge state={job.state} /></div>
            <div style={{ marginTop: 2, fontSize: 9, color: 'var(--text-muted)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{job.resource.display_name} · {job.id.slice(0, 8)}</div>
          </div>
        </NativeRow>
      ))}
    </NativePanelShell>
  )
}

export default function NativeJobsPanel() {
  const role = useAuthStore(state => state.user?.role)
  if (!roleAllowed(role, OPERATOR_ROLES)) return <NativePanelShell><EmptyState text="Your role cannot open durable jobs" /></NativePanelShell>
  return <JobsPanelContent />
}
