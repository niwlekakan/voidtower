import { useCallback } from 'react'
import { api } from '@/api/client'
import type { DurableApproval, DurableApprovalStatus, DurableJob } from '@/api/types'
import { ACTIVE_JOB_STATES } from '@/operations/state'
import { useBoundedPolling } from './useBoundedPolling'

const DETAIL_POLL_MS = 2_000
const LIST_POLL_MS = 5_000

export function useJobList(limit = 50) {
  const load = useCallback(async () => (await api.operationJobs.list(limit)).jobs, [limit])
  const shouldPoll = useCallback((jobs: DurableJob[]) => jobs.some(job => ACTIVE_JOB_STATES.has(job.state)), [])
  const eventPredicate = useCallback((event: { job_id: string | null }) => Boolean(event.job_id), [])
  return useBoundedPolling({ key: `jobs:${limit}`, intervalMs: LIST_POLL_MS, load, shouldPoll, eventPredicate })
}

export function useJobDetail(id: string | undefined) {
  const load = useCallback(async () => {
    if (!id) throw new Error('A job ID is required.')
    return (await api.operationJobs.get(id)).job
  }, [id])
  const shouldPoll = useCallback((job: DurableJob) => ACTIVE_JOB_STATES.has(job.state), [])
  const eventPredicate = useCallback((event: { job_id: string | null }) => event.job_id === id, [id])
  return useBoundedPolling({
    key: `job:${id ?? ''}`,
    enabled: Boolean(id),
    intervalMs: DETAIL_POLL_MS,
    load,
    shouldPoll,
    eventPredicate,
  })
}

export function useApprovalList(status?: DurableApprovalStatus, limit = 50) {
  const load = useCallback(async () => (await api.approvals.list({ status, limit })).approvals, [limit, status])
  const shouldPoll = useCallback((approvals: DurableApproval[]) => approvals.some(item => item.status === 'pending'), [])
  const eventPredicate = useCallback((event: { approval_id: string | null }) => Boolean(event.approval_id), [])
  return useBoundedPolling({
    key: `approvals:${status ?? 'all'}:${limit}`,
    intervalMs: LIST_POLL_MS,
    load,
    shouldPoll,
    eventPredicate,
  })
}

export function useApprovalDetail(id: string | undefined) {
  const load = useCallback(async () => {
    if (!id) throw new Error('An approval ID is required.')
    return (await api.approvals.get(id)).approval
  }, [id])
  const shouldPoll = useCallback((approval: DurableApproval) => approval.status === 'pending', [])
  const eventPredicate = useCallback((event: { approval_id: string | null }) => event.approval_id === id, [id])
  return useBoundedPolling({
    key: `approval:${id ?? ''}`,
    enabled: Boolean(id),
    intervalMs: LIST_POLL_MS,
    load,
    shouldPoll,
    eventPredicate,
  })
}
