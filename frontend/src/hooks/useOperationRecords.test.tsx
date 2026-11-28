import { renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { DurableEventEnvelope } from '@/api/types'
import { useBoundedPolling } from './useBoundedPolling'
import { useApprovalDetail, useApprovalList, useJobDetail, useJobList } from './useOperationRecords'

vi.mock('./useBoundedPolling', () => ({
  useBoundedPolling: vi.fn(() => ({
    data: null,
    loading: false,
    refreshing: false,
    stale: false,
    error: null,
    deadlineReached: false,
    refresh: vi.fn(),
    accept: vi.fn(),
  })),
}))

function event(jobId: string | null, approvalId: string | null): DurableEventEnvelope {
  return {
    sequence: 1,
    event_id: 'event-1',
    schema_version: 1,
    event_type: 'test.event.v1',
    occurred_at: 1,
    actor: null,
    resource_id: null,
    job_id: jobId,
    approval_id: approvalId,
    correlation_id: 'correlation-1',
    causation_id: null,
    payload: {},
  }
}

function predicate() {
  const options = vi.mocked(useBoundedPolling).mock.calls[0][0]
  expect(options.eventPredicate).toBeTypeOf('function')
  return options.eventPredicate!
}

describe('operation record durable invalidation identities', () => {
  beforeEach(() => vi.mocked(useBoundedPolling).mockClear())

  it('invalidates job lists broadly and job details by exact identity', () => {
    renderHook(() => useJobList())
    expect(predicate()(event('job-1', null))).toBe(true)
    expect(predicate()(event(null, 'approval-1'))).toBe(false)

    vi.mocked(useBoundedPolling).mockClear()
    renderHook(() => useJobDetail('job-1'))
    expect(predicate()(event('job-1', null))).toBe(true)
    expect(predicate()(event('job-2', null))).toBe(false)
  })

  it('invalidates approval lists broadly and details by exact identity', () => {
    renderHook(() => useApprovalList('pending'))
    expect(predicate()(event(null, 'approval-1'))).toBe(true)
    expect(predicate()(event('job-1', null))).toBe(false)

    vi.mocked(useBoundedPolling).mockClear()
    renderHook(() => useApprovalDetail('approval-1'))
    expect(predicate()(event(null, 'approval-1'))).toBe(true)
    expect(predicate()(event(null, 'approval-2'))).toBe(false)
  })
})
