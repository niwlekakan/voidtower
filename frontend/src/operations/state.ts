import type { DurableApprovalStatus, DurableJobState } from '@/api/types'

export const ACTIVE_JOB_STATES: ReadonlySet<DurableJobState> = new Set([
  'awaiting_approval',
  'queued',
  'running',
])

export function jobStateLabel(state: DurableJobState): string {
  switch (state) {
    case 'awaiting_approval': return 'Awaiting approval'
    case 'queued': return 'Queued'
    case 'running': return 'Running'
    case 'succeeded': return 'Succeeded'
    case 'failed': return 'Failed'
    case 'cancelled': return 'Cancelled'
    case 'needs_attention': return 'Needs attention'
    case 'rejected': return 'Rejected'
    case 'expired': return 'Expired'
  }
}

export type OperationTone = 'info' | 'success' | 'warning' | 'error' | 'muted'

export function jobStateTone(state: DurableJobState): OperationTone {
  switch (state) {
    case 'succeeded': return 'success'
    case 'failed':
    case 'rejected': return 'error'
    case 'awaiting_approval':
    case 'cancelled':
    case 'needs_attention':
    case 'expired': return 'warning'
    case 'queued':
    case 'running': return 'info'
  }
}

export function approvalStateLabel(state: DurableApprovalStatus): string {
  return state.charAt(0).toUpperCase() + state.slice(1)
}

export function approvalStateTone(state: DurableApprovalStatus): OperationTone {
  switch (state) {
    case 'approved': return 'success'
    case 'rejected': return 'error'
    case 'pending': return 'warning'
    case 'expired':
    case 'stale': return 'muted'
  }
}
