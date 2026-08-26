import { useCallback, useEffect, useRef, useState } from 'react'

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
  foregroundLimitMs = OPERATION_FOREGROUND_LIMIT_MS,
}: Options<T>): BoundedPollingResult<T> {
  const loadRef = useRef(load)
  const shouldPollRef = useRef(shouldPoll)
  const manualRefreshRef = useRef<() => Promise<T | null>>(async () => null)
  const acceptRef = useRef<(value: T) => void>(() => {})
  loadRef.current = load
  shouldPollRef.current = shouldPoll

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
      if (cancelled) return
      if (Date.now() >= deadline) {
        setDeadlineReached(true)
        return
      }
      const continuePolling = current === null || shouldPollRef.current(current)
      if (!continuePolling) return
      timer = setTimeout(tick, intervalMs)
    }

    const tick = async () => {
      if (cancelled) return
      if (document.hidden) {
        schedule()
        return
      }
      await fetchFull(false)
      schedule()
    }

    const onVisibility = () => {
      if (document.hidden || cancelled) return
      if (timer) clearTimeout(timer)
      void tick()
    }

    manualRefreshRef.current = () => fetchFull(false)
    acceptRef.current = value => {
      current = value
      setData(value)
      setStale(false)
      setError(null)
      if (timer && !shouldPollRef.current(value)) {
        clearTimeout(timer)
        timer = undefined
      }
    }
    document.addEventListener('visibilitychange', onVisibility)
    void fetchFull(true).then(schedule)

    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
      document.removeEventListener('visibilitychange', onVisibility)
    }
  }, [enabled, foregroundLimitMs, intervalMs, key])

  const refresh = useCallback(() => manualRefreshRef.current(), [])
  const accept = useCallback((value: T) => acceptRef.current(value), [])

  return { data, loading, refreshing, stale, error, deadlineReached, refresh, accept }
}
