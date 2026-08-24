import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '@/api/client'
import type { DurableJobState, DurableJobSummary } from '@/api/types'
import { notify } from '@/store/notifications'

const POLL_MILLIS = 2_000
const FOREGROUND_LIMIT_MILLIS = 20 * 60_000

const FOLLOWED_STATES: ReadonlySet<DurableJobState> = new Set([
  'awaiting_approval',
  'queued',
  'running',
])

export type DurableJobTone = 'info' | 'success' | 'warning' | 'error'

interface TrackOptions {
  label: string
  onSucceeded?: () => void | Promise<void>
}

interface TrackedJob {
  job: DurableJobSummary
  label: string
  deadline: number
  foregroundStopped: boolean
}

interface CompletionCallbacks {
  jobId: string
  onSucceeded?: () => void | Promise<void>
}

export function durableJobStateLabel(state: DurableJobState): string {
  switch (state) {
    case 'awaiting_approval': return 'awaiting approval'
    case 'queued': return 'queued'
    case 'running': return 'running'
    case 'succeeded': return 'succeeded'
    case 'failed': return 'failed'
    case 'cancelled': return 'cancelled'
    case 'needs_attention': return 'needs attention'
    case 'rejected': return 'rejected'
    case 'expired': return 'expired'
  }
}

export function durableJobTone(state: DurableJobState): DurableJobTone {
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

function announceTerminal(label: string, job: DurableJobSummary) {
  const shortId = job.id.slice(0, 8)
  const message = `Job ${shortId} · ${durableJobStateLabel(job.state)}`
  switch (durableJobTone(job.state)) {
    case 'success': notify.success(`${label} completed`, message); break
    case 'error': notify.error(`${label} did not complete`, message); break
    case 'warning': notify.warning(`${label} requires attention`, message); break
    case 'info': notify.info(label, message)
  }
}

export function useDurableJobTracker() {
  const [tracked, setTracked] = useState<TrackedJob | null>(null)
  const callbacks = useRef<CompletionCallbacks | null>(null)

  const track = useCallback((job: DurableJobSummary, options: TrackOptions) => {
    callbacks.current = { jobId: job.id, onSucceeded: options.onSucceeded }
    setTracked({
      job,
      label: options.label,
      deadline: Date.now() + FOREGROUND_LIMIT_MILLIS,
      foregroundStopped: false,
    })
    if (!FOLLOWED_STATES.has(job.state)) {
      announceTerminal(options.label, job)
      callbacks.current = null
      if (job.state === 'succeeded') void options.onSucceeded?.()
      return
    }
    const shortId = job.id.slice(0, 8)
    if (job.state === 'awaiting_approval') {
      notify.warning(`${options.label} awaiting approval`, `Job ${shortId}`)
    } else {
      notify.info(`${options.label} submitted`, `Job ${shortId} · ${durableJobStateLabel(job.state)}`)
    }
  }, [])

  const clear = useCallback(() => {
    callbacks.current = null
    setTracked(null)
  }, [])

  useEffect(() => {
    if (!tracked || tracked.foregroundStopped || !FOLLOWED_STATES.has(tracked.job.state)) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined

    const schedule = () => {
      timer = setTimeout(poll, POLL_MILLIS)
    }

    const poll = async () => {
      if (cancelled) return
      if (Date.now() >= tracked.deadline) {
        notify.warning(
          `${tracked.label} is still running durably`,
          `Stopped foreground tracking for job ${tracked.job.id.slice(0, 8)}`,
        )
        callbacks.current = null
        setTracked((current) => current?.job.id === tracked.job.id
          ? { ...current, foregroundStopped: true }
          : current)
        return
      }
      try {
        const { job } = await api.operationJobs.get(tracked.job.id)
        if (cancelled) return
        setTracked((current) => current?.job.id === job.id ? { ...current, job } : current)
        if (FOLLOWED_STATES.has(job.state)) {
          schedule()
          return
        }
        announceTerminal(tracked.label, job)
        const completion = callbacks.current
        callbacks.current = null
        if (job.state === 'succeeded' && completion?.jobId === job.id) {
          await completion.onSucceeded?.()
        }
      } catch {
        if (!cancelled) schedule()
      }
    }

    schedule()
    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
    }
  }, [tracked])

  return {
    trackedJob: tracked?.job ?? null,
    trackedLabel: tracked?.label ?? null,
    tracking: tracked
      ? !tracked.foregroundStopped && FOLLOWED_STATES.has(tracked.job.state)
      : false,
    track,
    clear,
  }
}
