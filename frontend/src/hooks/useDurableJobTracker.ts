import { useCallback, useEffect, useRef, useState } from 'react'
import { api } from '@/api/client'
import type { DurableJobState, DurableJobSummary } from '@/api/types'
import { notify } from '@/store/notifications'
import { ACTIVE_JOB_STATES, jobStateLabel, jobStateTone } from '@/operations/state'

const POLL_MILLIS = 2_000
const FOREGROUND_LIMIT_MILLIS = 20 * 60_000

const FOLLOWED_STATES: ReadonlySet<DurableJobState> = ACTIVE_JOB_STATES

export type DurableJobTone = 'info' | 'success' | 'warning' | 'error'

interface TrackOptions {
  label: string
  onSucceeded?: (job: DurableJobSummary) => void | Promise<void>
}

interface TrackedJob {
  job: DurableJobSummary
  label: string
  deadline: number
  foregroundStopped: boolean
}

interface CompletionCallbacks {
  jobId: string
  onSucceeded?: (job: DurableJobSummary) => void | Promise<void>
}

export function durableJobStateLabel(state: DurableJobState): string {
  return jobStateLabel(state).toLowerCase()
}

export function durableJobTone(state: DurableJobState): DurableJobTone {
  const tone = jobStateTone(state)
  return tone === 'muted' ? 'warning' : tone
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
      if (job.state === 'succeeded') void options.onSucceeded?.(job)
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
          await completion.onSucceeded?.(job)
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

interface BatchCallbacks {
  onSucceeded?: () => void | Promise<void>
}

export function useDurableJobBatchTracker() {
  const [batch, setBatch] = useState<{
    jobs: DurableJobSummary[]
    label: string
    deadline: number
    foregroundStopped: boolean
  } | null>(null)
  const callbacks = useRef<BatchCallbacks | null>(null)

  const trackBatch = useCallback((jobs: DurableJobSummary[], options: {
    label: string
    onSucceeded?: () => void | Promise<void>
  }) => {
    if (jobs.length === 0) return
    callbacks.current = { onSucceeded: options.onSucceeded }
    setBatch({
      jobs,
      label: options.label,
      deadline: Date.now() + FOREGROUND_LIMIT_MILLIS,
      foregroundStopped: false,
    })
    notify.info(`${options.label} submitted`, `${jobs.length} durable jobs`)
  }, [])

  useEffect(() => {
    if (!batch || batch.foregroundStopped || batch.jobs.every(job => !FOLLOWED_STATES.has(job.state))) return
    let cancelled = false
    let timer: ReturnType<typeof setTimeout> | undefined
    const poll = async () => {
      if (cancelled) return
      if (Date.now() >= batch.deadline) {
        callbacks.current = null
        setBatch(current => current ? { ...current, foregroundStopped: true } : current)
        notify.warning(`${batch.label} is still running durably`, 'Stopped foreground batch tracking')
        return
      }
      try {
        const responses = await Promise.all(batch.jobs.map(job => api.operationJobs.get(job.id)))
        if (cancelled) return
        const jobs = responses.map(response => response.job)
        setBatch(current => current ? { ...current, jobs } : current)
        if (jobs.some(job => FOLLOWED_STATES.has(job.state))) {
          timer = setTimeout(poll, POLL_MILLIS)
          return
        }
        const succeeded = jobs.filter(job => job.state === 'succeeded').length
        if (succeeded === jobs.length) {
          notify.success(`${batch.label} completed`, `${succeeded} of ${jobs.length} jobs succeeded`)
          const completion = callbacks.current
          callbacks.current = null
          await completion?.onSucceeded?.()
        } else {
          callbacks.current = null
          notify.warning(`${batch.label} requires attention`, `${succeeded} of ${jobs.length} jobs succeeded`)
        }
      } catch {
        if (!cancelled) timer = setTimeout(poll, POLL_MILLIS)
      }
    }
    timer = setTimeout(poll, POLL_MILLIS)
    return () => {
      cancelled = true
      if (timer) clearTimeout(timer)
    }
  }, [batch])

  return {
    batchJobs: batch?.jobs ?? [],
    batchLabel: batch?.label ?? null,
    batchTracking: batch
      ? !batch.foregroundStopped && batch.jobs.some(job => FOLLOWED_STATES.has(job.state))
      : false,
    trackBatch,
  }
}
