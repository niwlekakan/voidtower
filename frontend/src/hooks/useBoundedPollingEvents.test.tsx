import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DurableEventEnvelope } from '@/api/types'
import type { DurableEventSubscriber } from '@/operations/durableEvents'

const stream = vi.hoisted(() => ({ subscriber: null as DurableEventSubscriber | null }))

vi.mock('@/operations/durableEvents', () => ({
  subscribeDurableEvents: vi.fn((subscriber: DurableEventSubscriber) => {
    stream.subscriber = subscriber
    subscriber.onState({ status: 'connecting', cursor: null, gap: null })
    return () => { stream.subscriber = null }
  }),
}))

import { useBoundedPolling } from './useBoundedPolling'

interface Value { active: boolean; version: number }

function event(jobId = 'job-1'): DurableEventEnvelope {
  return {
    sequence: 1,
    event_id: 'event-1',
    schema_version: 1,
    event_type: 'job.running.v1',
    occurred_at: 1,
    actor: null,
    resource_id: 'resource-1',
    job_id: jobId,
    approval_id: null,
    correlation_id: 'correlation-1',
    causation_id: null,
    payload: {},
  }
}

describe('useBoundedPolling durable invalidation fallback', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    stream.subscriber = null
    Object.defineProperty(document, 'hidden', { configurable: true, value: false })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('uses a full-read ready barrier, pauses intervals, and resumes them after disconnect', async () => {
    let version = 0
    const load = vi.fn<() => Promise<Value>>(async () => ({ active: true, version: ++version }))
    const { result } = renderHook(() => useBoundedPolling({
      key: 'stream-record',
      intervalMs: 100,
      foregroundLimitMs: 1_000,
      load,
      shouldPoll: value => value.active,
      eventPredicate: value => value.job_id === 'job-1',
    }))
    await act(async () => { await Promise.resolve() })
    expect(load).toHaveBeenCalledTimes(1)

    await act(async () => {
      stream.subscriber?.onState({ status: 'ready', cursor: 0, gap: null })
      await Promise.resolve()
    })
    expect(load).toHaveBeenCalledTimes(2)
    expect(result.current.data?.version).toBe(2)
    await act(async () => { await vi.advanceTimersByTimeAsync(400) })
    expect(load).toHaveBeenCalledTimes(2)

    await act(async () => {
      stream.subscriber?.onEvent(event('other-job'))
      await Promise.resolve()
    })
    expect(load).toHaveBeenCalledTimes(2)
    await act(async () => {
      stream.subscriber?.onEvent(event())
      await Promise.resolve()
    })
    expect(load).toHaveBeenCalledTimes(3)

    act(() => stream.subscriber?.onState({ status: 'disconnected', cursor: 1, gap: null }))
    await act(async () => { await vi.advanceTimersByTimeAsync(100) })
    expect(load).toHaveBeenCalledTimes(4)
  })

  it('coalesces an event burst into one pending authoritative read', async () => {
    let resolveBarrier: ((value: Value) => void) | undefined
    const load = vi.fn<() => Promise<Value>>()
      .mockResolvedValueOnce({ active: true, version: 1 })
      .mockImplementationOnce(() => new Promise(resolve => { resolveBarrier = resolve }))
      .mockResolvedValue({ active: true, version: 3 })
    renderHook(() => useBoundedPolling({
      key: 'coalesced-record',
      intervalMs: 100,
      foregroundLimitMs: 1_000,
      load,
      shouldPoll: value => value.active,
      eventPredicate: value => value.job_id === 'job-1',
    }))
    await act(async () => { await Promise.resolve() })
    act(() => stream.subscriber?.onState({ status: 'ready', cursor: 0, gap: null }))
    expect(load).toHaveBeenCalledTimes(2)

    act(() => {
      stream.subscriber?.onEvent(event())
      stream.subscriber?.onEvent(event())
      stream.subscriber?.onEvent(event())
    })
    await act(async () => {
      resolveBarrier?.({ active: true, version: 2 })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(load).toHaveBeenCalledTimes(3)
  })

  it('resumes bounded polling when an event-triggered authoritative read fails', async () => {
    const load = vi.fn<() => Promise<Value>>()
      .mockResolvedValueOnce({ active: true, version: 1 })
      .mockResolvedValueOnce({ active: true, version: 2 })
      .mockRejectedValueOnce(new Error('read gap'))
      .mockResolvedValue({ active: true, version: 4 })
    const { result } = renderHook(() => useBoundedPolling({
      key: 'failed-invalidation',
      intervalMs: 100,
      foregroundLimitMs: 1_000,
      load,
      shouldPoll: value => value.active,
      eventPredicate: value => value.job_id === 'job-1',
    }))
    await act(async () => { await Promise.resolve() })
    await act(async () => {
      stream.subscriber?.onState({ status: 'ready', cursor: 0, gap: null })
      await Promise.resolve()
    })
    await act(async () => {
      stream.subscriber?.onEvent(event())
      await Promise.resolve()
    })
    expect(result.current.stale).toBe(true)
    expect(result.current.error).toBe('read gap')

    await act(async () => { await vi.advanceTimersByTimeAsync(100) })
    expect(load).toHaveBeenCalledTimes(4)
    expect(result.current.stale).toBe(false)
  })

  it('forces an immediate authoritative read when the stream reports a cursor gap', async () => {
    const load = vi.fn<() => Promise<Value>>().mockResolvedValue({ active: true, version: 1 })
    renderHook(() => useBoundedPolling({
      key: 'gap-recovery',
      intervalMs: 100,
      foregroundLimitMs: 1_000,
      load,
      shouldPoll: value => value.active,
      eventPredicate: value => value.job_id === 'job-1',
    }))
    await act(async () => { await Promise.resolve() })
    expect(load).toHaveBeenCalledTimes(1)

    await act(async () => {
      stream.subscriber?.onState({
        status: 'gap',
        cursor: null,
        gap: {
          reason: 'behind_retention',
          requested_after: 2,
          earliest_available: 5,
          latest_available: 9,
        },
      })
      await Promise.resolve()
    })
    expect(load).toHaveBeenCalledTimes(2)
  })

  it('forces visibility recovery and keeps the original foreground deadline', async () => {
    const load = vi.fn<() => Promise<Value>>().mockResolvedValue({ active: true, version: 1 })
    const { result } = renderHook(() => useBoundedPolling({
      key: 'visibility-record',
      intervalMs: 100,
      foregroundLimitMs: 150,
      load,
      shouldPoll: value => value.active,
      eventPredicate: value => value.job_id === 'job-1',
    }))
    await act(async () => { await Promise.resolve() })
    await act(async () => {
      stream.subscriber?.onState({ status: 'ready', cursor: 0, gap: null })
      await Promise.resolve()
    })
    expect(load).toHaveBeenCalledTimes(2)

    Object.defineProperty(document, 'hidden', { configurable: true, value: true })
    await act(async () => { await vi.advanceTimersByTimeAsync(100) })
    Object.defineProperty(document, 'hidden', { configurable: true, value: false })
    await act(async () => {
      document.dispatchEvent(new Event('visibilitychange'))
      await Promise.resolve()
    })
    expect(load).toHaveBeenCalledTimes(3)

    await act(async () => { await vi.advanceTimersByTimeAsync(100) })
    act(() => stream.subscriber?.onEvent(event()))
    expect(result.current.deadlineReached).toBe(true)
    expect(load).toHaveBeenCalledTimes(3)
  })
})
