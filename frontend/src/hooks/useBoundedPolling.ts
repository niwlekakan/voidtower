import { useCallback, useEffect, useRef, useState } from 'react'
import type { DurableEventEnvelope } from '@/api/types'
import { subscribeDurableEvents } from '@/operations/durableEvents'

export const OPERATION_FOREGROUND_LIMIT_MS = 20 * 60_000

export interface BoundedPollingResult<T> {
  data: T | null
  loading: boolean
  refreshing: boolean
  stale: boolean
  error: string | null
  deadlineReached: boolean
  refresh: () => Promise<T | null>
  accept: (value: T) => void
}

interface Options<T> {
  key: string
  enabled?: boolean
  intervalMs: number
  load: () => Promise<T>
  shouldPoll: (value: T) => boolean
  eventPredicate?: (event: DurableEventEnvelope) => boolean
  foregroundLimitMs?: number
}

function errorMessage(error: unknown): string {
  return error instanceof Error && error.message ? error.message : 'Unable to refresh this record.'
}

export function useBoundedPolling<T>({
  key,
  enabled = true,
  intervalMs,
  load,
  shouldPoll,
  eventPredicate,
  foregroundLimitMs = OPERATION_FOREGROUND_LIMIT_MS,
}: Options<T>): BoundedPollingResult<T> {
  const loadRef = useRef(load)
  const shouldPollRef = useRef(shouldPoll)
  const eventPredicateRef = useRef(eventPredicate)
  const manualRefreshRef = useRef<() => Promise<T | null>>(async () => null)
  const acceptRef = useRef<(value: T) => void>(() => {})
  loadRef.current = load
  shouldPollRef.current = shouldPoll
  eventPredicateRef.current = eventPredicate

  const [data, setData] = useState<T | null>(null)
  const [loading, setLoading] = useState(enabled)
  const [refreshing, setRefreshing] = useState(false)
  const [stale, setStale] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [deadlineReached, setDeadlineReached] = useState(false)

  useEffect(() => {
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    let current: T | null = null
    let transportReady = false
    let streamReady = false
    let activeFetch: Promise<T | null> | null = null
    let pendingFetch = false
    let pendingBarrier = false
    let unsubscribe = () => {}
    const deadline = Date.now() + foregroundLimitMs

    setData(null)
    setLoading(enabled)
    setRefreshing(false)
    setStale(false)
    setError(null)
    setDeadlineReached(false)

    if (!enabled) {
      manualRefreshRef.current = async () => null
      acceptRef.current = () => {}
      return
    }

    const clearTimer = () => {
      if (timer) clearTimeout(timer)
      timer = undefined
    }

    const automaticRefreshAllowed = () => {
      if (Date.now() < deadline) return true
      setDeadlineReached(true)
      return false
    }

    const fetchFull = async (initial: boolean): Promise<T | null> => {
      if (cancelled) return null
      if (initial) setLoading(true)
      else setRefreshing(true)
      try {
        const value = await loadRef.current()
        if (cancelled) return null
        current = value
        setData(value)
        setStale(false)
        setError(null)
        return value
      } catch (cause) {
        if (cancelled) return null
        setError(errorMessage(cause))
        setStale(current !== null)
        return null
      } finally {
        if (!cancelled) {
          setLoading(false)
          setRefreshing(false)
        }
      }
    }

    const schedule = () => {
      clearTimer()
      if (cancelled || streamReady) return
      if (!automaticRefreshAllowed()) return
      const continuePolling = current === null || shouldPollRef.current(current)
      if (!continuePolling) return
      timer = setTimeout(tick, intervalMs)
    }

    const requestFetch = (initial: boolean, barrier: boolean): Promise<T | null> => {
      if (cancelled) return Promise.resolve(null)
      if (activeFetch) {
        pendingFetch = true
        pendingBarrier ||= barrier
        return activeFetch
      }

      activeFetch = fetchFull(initial)
        .then(value => {
          if (barrier) streamReady = value !== null && transportReady
          return value
        })
        .finally(() => {
          activeFetch = null
          if (pendingFetch && !cancelled) {
            const nextBarrier = pendingBarrier
            pendingFetch = false
            pendingBarrier = false
            void requestFetch(false, nextBarrier).then(schedule)
          }
        })
      return activeFetch
    }

    async function tick() {
      timer = undefined
      if (cancelled) return
      if (document.hidden) {
        schedule()
        return
      }
      if (!automaticRefreshAllowed()) return
      await requestFetch(false, false)
      schedule()
    }

    const onVisibility = () => {
      if (document.hidden || cancelled) return
      clearTimer()
      streamReady = false
      void requestFetch(false, transportReady).then(schedule)
    }

    manualRefreshRef.current = () => requestFetch(false, false)
    acceptRef.current = value => {
      current = value
      setData(value)
      setStale(false)
      setError(null)
      if (!shouldPollRef.current(value)) clearTimer()
    }

    if (eventPredicateRef.current) {
      unsubscribe = subscribeDurableEvents({
        onEvent: event => {
          if (!eventPredicateRef.current?.(event) || document.hidden || !automaticRefreshAllowed()) return
          streamReady = false
          clearTimer()
          void requestFetch(false, true).then(schedule)
        },
        onState: next => {
          transportReady = next.status === 'ready'
          streamReady = false
          if (!transportReady) {
            if (next.status === 'gap' && !document.hidden && automaticRefreshAllowed()) {
              clearTimer()
              void requestFetch(false, false).then(schedule)
              return
            }
            schedule()
            return
          }
          clearTimer()
          if (document.hidden || !automaticRefreshAllowed()) {
            schedule()
            return
          }
          void requestFetch(false, true).then(schedule)
        },
      })
    }

    document.addEventListener('visibilitychange', onVisibility)
    void requestFetch(true, false).then(schedule)

    return () => {
      cancelled = true
      clearTimer()
      unsubscribe()
      document.removeEventListener('visibilitychange', onVisibility)
    }
  }, [enabled, foregroundLimitMs, intervalMs, key])

  const refresh = useCallback(() => manualRefreshRef.current(), [])
  const accept = useCallback((value: T) => acceptRef.current(value), [])

  return { data, loading, refreshing, stale, error, deadlineReached, refresh, accept }
}
