import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DurableEventEnvelope } from '@/api/types'
import {
  resetDurableEventStreamForTests,
  subscribeDurableEvents,
  type DurableEventConnectionState,
} from './durableEvents'

type EventListener = (event: Event) => void

class FakeEventSource {
  static instances: FakeEventSource[] = []

  readonly url: string
  readonly withCredentials: boolean
  onerror: ((event: Event) => void) | null = null
  closed = false
  private listeners = new Map<string, Set<EventListener>>()

  constructor(url: string | URL, init?: EventSourceInit) {
    this.url = String(url)
    this.withCredentials = init?.withCredentials ?? false
    FakeEventSource.instances.push(this)
  }

  addEventListener(type: string, listener: EventListenerOrEventListenerObject | null) {
    if (!listener) return
    const callback: EventListener = typeof listener === 'function'
      ? listener as EventListener
      : event => listener.handleEvent(event)
    const listeners = this.listeners.get(type) ?? new Set<EventListener>()
    listeners.add(callback)
    this.listeners.set(type, listeners)
  }

  close() {
    this.closed = true
  }

  emit(type: string, data: unknown, lastEventId = '') {
    const event = new MessageEvent(type, {
      data: typeof data === 'string' ? data : JSON.stringify(data),
      lastEventId,
    })
    for (const listener of this.listeners.get(type) ?? []) listener(event)
  }

  fail() {
    this.onerror?.(new Event('error'))
  }
}

function envelope(sequence: number): DurableEventEnvelope {
  return {
    sequence,
    event_id: `event-${sequence}`,
    schema_version: 1,
    event_type: 'job.running.v1',
    occurred_at: 100 + sequence,
    actor: null,
    resource_id: 'resource-1',
    job_id: 'job-1',
    approval_id: null,
    correlation_id: 'correlation-1',
    causation_id: null,
    payload: { state: 'running' },
  }
}

describe('durable event stream client', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    FakeEventSource.instances = []
    vi.stubGlobal('EventSource', FakeEventSource)
    resetDurableEventStreamForTests()
  })

  afterEach(() => {
    resetDurableEventStreamForTests()
    vi.unstubAllGlobals()
    vi.useRealTimers()
  })

  it('shares one source and publishes only validated monotonic envelopes', () => {
    const states: DurableEventConnectionState[] = []
    const events: DurableEventEnvelope[] = []
    const first = subscribeDurableEvents({ onState: state => states.push(state), onEvent: event => events.push(event) })
    const second = subscribeDurableEvents({ onState: () => {}, onEvent: () => {} })

    expect(FakeEventSource.instances).toHaveLength(1)
    const source = FakeEventSource.instances[0]
    expect(source.url).toBe('/api/events/stream')
    expect(source.withCredentials).toBe(true)
    source.emit('stream.ready', { cursor: 0, high_water: 0 })
    source.emit('durable_event', envelope(1), '1')

    expect(states[states.length - 1]).toMatchObject({ status: 'ready', cursor: 0 })
    expect(events).toEqual([envelope(1)])
    first()
    expect(source.closed).toBe(false)
    second()
    expect(source.closed).toBe(true)
  })

  it('rejects incompatible or unbounded event metadata as a gap', async () => {
    const states: DurableEventConnectionState[] = []
    const unsubscribe = subscribeDurableEvents({ onState: state => states.push(state), onEvent: () => {} })
    const source = FakeEventSource.instances[0]
    source.emit('stream.ready', { cursor: 0, high_water: 0 })
    source.emit('durable_event', { ...envelope(1), schema_version: 2 }, '1')

    expect(states[states.length - 1]?.status).toBe('gap')
    expect(states[states.length - 1]?.gap?.reason).toBe('invalid_frame')
    unsubscribe()
  })

  it('treats malformed and non-monotonic frames as gaps and resumes from the last valid ID', async () => {
    const states: DurableEventConnectionState[] = []
    const unsubscribe = subscribeDurableEvents({ onState: state => states.push(state), onEvent: () => {} })
    const source = FakeEventSource.instances[0]
    source.emit('stream.ready', { cursor: 0, high_water: 0 })
    source.emit('durable_event', envelope(1), '1')
    source.emit('durable_event', envelope(3), '3')

    expect(source.closed).toBe(true)
    expect(states[states.length - 1]?.status).toBe('gap')
    expect(states[states.length - 1]?.gap?.reason).toBe('invalid_frame')
    await vi.runOnlyPendingTimersAsync()
    expect(FakeEventSource.instances[FakeEventSource.instances.length - 1]?.url).toBe('/api/events/stream?after=1')
    unsubscribe()
  })

  it('reconnects from the last accepted cursor after transport disconnect', async () => {
    const states: DurableEventConnectionState[] = []
    const unsubscribe = subscribeDurableEvents({ onState: state => states.push(state), onEvent: () => {} })
    const source = FakeEventSource.instances[0]
    source.emit('stream.ready', { cursor: 0, high_water: 0 })
    source.emit('durable_event', envelope(1), '1')
    source.fail()

    expect(states[states.length - 1]).toMatchObject({ status: 'disconnected', cursor: 1 })
    expect(source.closed).toBe(true)
    await vi.advanceTimersByTimeAsync(999)
    expect(FakeEventSource.instances).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(1)

    const replacement = FakeEventSource.instances[FakeEventSource.instances.length - 1]!
    expect(replacement.url).toBe('/api/events/stream?after=1')
    expect(replacement.withCredentials).toBe(true)
    unsubscribe()
  })

  it('uses server gap high-water for reset and exposes transport disconnects', async () => {
    const states: DurableEventConnectionState[] = []
    const unsubscribe = subscribeDurableEvents({ onState: state => states.push(state), onEvent: () => {} })
    const source = FakeEventSource.instances[0]
    source.emit('stream.gap', {
      reason: 'behind_retention',
      requested_after: 2,
      earliest_available: 5,
      latest_available: 9,
    })
    expect(states[states.length - 1]?.status).toBe('gap')
    await vi.runOnlyPendingTimersAsync()

    const replacement = FakeEventSource.instances[FakeEventSource.instances.length - 1]!
    expect(replacement.url).toBe('/api/events/stream?after=9')
    replacement.fail()
    expect(states[states.length - 1]).toMatchObject({ status: 'disconnected', cursor: null })
    expect(replacement.closed).toBe(true)
    unsubscribe()
  })
})
