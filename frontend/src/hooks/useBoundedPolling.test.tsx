import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useBoundedPolling } from './useBoundedPolling'

interface Value { active: boolean; version: number }

describe('useBoundedPolling', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    Object.defineProperty(document, 'hidden', { configurable: true, value: false })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('polls active data and stops after the confirmed terminal value', async () => {
    const load = vi.fn<() => Promise<Value>>()
      .mockResolvedValueOnce({ active: true, version: 1 })
      .mockResolvedValueOnce({ active: false, version: 2 })
    const { result } = renderHook(() => useBoundedPolling({
      key: 'record', intervalMs: 100, foregroundLimitMs: 1_000, load,
      shouldPoll: value => value.active,
    }))
    await act(async () => { await Promise.resolve() })
    expect(result.current.data?.version).toBe(1)
    await act(async () => { await vi.advanceTimersByTimeAsync(100) })
    expect(result.current.data?.version).toBe(2)
    await act(async () => { await vi.advanceTimersByTimeAsync(500) })
    expect(load).toHaveBeenCalledTimes(2)
  })

  it('preserves confirmed data and marks it stale after a read gap', async () => {
    const load = vi.fn<() => Promise<Value>>()
      .mockResolvedValueOnce({ active: true, version: 1 })
      .mockRejectedValueOnce(new Error('network gap'))
    const { result } = renderHook(() => useBoundedPolling({
      key: 'record', intervalMs: 100, foregroundLimitMs: 1_000, load,
      shouldPoll: value => value.active,
    }))
    await act(async () => { await Promise.resolve() })
    await act(async () => { await vi.advanceTimersByTimeAsync(100) })
    expect(result.current.data?.version).toBe(1)
    expect(result.current.stale).toBe(true)
    expect(result.current.error).toBe('network gap')
  })

  it('pauses while hidden and performs a full refetch on visibility return', async () => {
    const load = vi.fn<() => Promise<Value>>().mockResolvedValue({ active: true, version: 1 })
    renderHook(() => useBoundedPolling({
      key: 'record', intervalMs: 100, foregroundLimitMs: 1_000, load,
      shouldPoll: value => value.active,
    }))
    await act(async () => { await Promise.resolve() })
    Object.defineProperty(document, 'hidden', { configurable: true, value: true })
    await act(async () => { await vi.advanceTimersByTimeAsync(100) })
    expect(load).toHaveBeenCalledTimes(1)
    Object.defineProperty(document, 'hidden', { configurable: true, value: false })
    await act(async () => { document.dispatchEvent(new Event('visibilitychange')); await Promise.resolve() })
    expect(load).toHaveBeenCalledTimes(2)
  })

  it('stops automatic refresh at the fixed foreground deadline', async () => {
    const load = vi.fn<() => Promise<Value>>().mockResolvedValue({ active: true, version: 1 })
    const { result } = renderHook(() => useBoundedPolling({
      key: 'record', intervalMs: 100, foregroundLimitMs: 150, load,
      shouldPoll: value => value.active,
    }))
    await act(async () => { await Promise.resolve() })
    await act(async () => { await vi.advanceTimersByTimeAsync(200) })
    expect(result.current.deadlineReached).toBe(true)
    const count = load.mock.calls.length
    await act(async () => { await vi.advanceTimersByTimeAsync(500) })
    expect(load).toHaveBeenCalledTimes(count)
  })
})
