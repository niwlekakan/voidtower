import { api } from '@/api/client'
import { API_V1_ENVELOPE_CONTRACT } from '@/api/generatedApiContract'
import type {
  DurableEventEnvelope,
  DurableEventStreamGap,
  DurableEventStreamReady,
} from '@/api/types'

export type DurableEventConnectionStatus = 'connecting' | 'ready' | 'disconnected' | 'gap'

export interface DurableEventConnectionState {
  status: DurableEventConnectionStatus
  cursor: number | null
  gap: DurableEventStreamGap | null
}

export interface DurableEventSubscriber {
  onEvent: (event: DurableEventEnvelope) => void
  onState: (state: DurableEventConnectionState) => void
}

const subscribers = new Set<DurableEventSubscriber>()
let source: EventSource | null = null
let reconnectTimer: ReturnType<typeof setTimeout> | null = null
let reconnectAttempt = 0
let lastSequence: number | null = null
let state: DurableEventConnectionState = { status: 'disconnected', cursor: null, gap: null }
const EXPECTED_EVENT_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.event_v1.schema_version
const MAX_EVENT_FIELD_LENGTH = 256
const INITIAL_RECONNECT_DELAY_MS = 1_000
const MAX_RECONNECT_DELAY_MS = 30_000

function isSequence(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0
}

function isBoundedString(value: unknown): value is string {
  return typeof value === 'string'
    && value.length > 0
    && value.trim().length > 0
    && value.length <= MAX_EVENT_FIELD_LENGTH
}

function parseReady(data: string): DurableEventStreamReady | null {
  try {
    const value = JSON.parse(data) as Partial<DurableEventStreamReady>
    return isSequence(value.cursor) && isSequence(value.high_water) && value.cursor <= value.high_water
      ? { cursor: value.cursor, high_water: value.high_water }
      : null
  } catch {
    return null
  }
}

function parseGap(data: string): DurableEventStreamGap | null {
  try {
    const value = JSON.parse(data) as Partial<DurableEventStreamGap>
    const validReason = value.reason === 'behind_retention'
      || value.reason === 'future_cursor'
      || value.reason === 'discontinuity'
    const validEarliest = value.earliest_available === null || isSequence(value.earliest_available)
    return validReason && isSequence(value.requested_after) && validEarliest && isSequence(value.latest_available)
      ? value as DurableEventStreamGap
      : null
  } catch {
    return null
  }
}

function parseEnvelope(message: MessageEvent<string>): DurableEventEnvelope | null {
  try {
    const value = JSON.parse(message.data) as Partial<DurableEventEnvelope>
    const id = Number(message.lastEventId)
    const actor = value.actor
    const validActor = actor === null || (
      typeof actor === 'object'
      && isBoundedString(actor.actor_type)
      && (actor.id === null || isBoundedString(actor.id))
      && (actor.source === null || isBoundedString(actor.source))
    )
    const valid = isSequence(value.sequence)
      && isSequence(id)
      && id === value.sequence
      && isBoundedString(value.event_id)
      && value.schema_version === EXPECTED_EVENT_SCHEMA_VERSION
      && isBoundedString(value.event_type)
      && isSequence(value.occurred_at)
      && validActor
      && isBoundedString(value.correlation_id)
      && (value.resource_id === null || isBoundedString(value.resource_id))
      && (value.job_id === null || isBoundedString(value.job_id))
      && (value.approval_id === null || isBoundedString(value.approval_id))
      && (value.causation_id === null || isBoundedString(value.causation_id))
      && Object.prototype.hasOwnProperty.call(value, 'payload')
    return valid ? value as DurableEventEnvelope : null
  } catch {
    return null
  }
}

function publishState(next: DurableEventConnectionState) {
  state = next
  for (const subscriber of subscribers) subscriber.onState(next)
}

function publishEvent(event: DurableEventEnvelope) {
  for (const subscriber of subscribers) subscriber.onEvent(event)
}

function clearReconnect() {
  if (reconnectTimer !== null) clearTimeout(reconnectTimer)
  reconnectTimer = null
}

function closeSource() {
  source?.close()
  source = null
}

function localGap(reconnectAfter?: number) {
  const confirmed = lastSequence ?? 0
  const gap: DurableEventStreamGap = {
    reason: 'invalid_frame',
    requested_after: confirmed,
    earliest_available: null,
    latest_available: reconnectAfter ?? confirmed,
  }
  closeSource()
  lastSequence = null
  publishState({ status: 'gap', cursor: null, gap })
  scheduleReconnect(reconnectAfter)
}

function scheduleReconnect(after?: number) {
  clearReconnect()
  if (subscribers.size === 0) return
  const delay = Math.min(
    INITIAL_RECONNECT_DELAY_MS * (2 ** reconnectAttempt),
    MAX_RECONNECT_DELAY_MS,
  )
  reconnectAttempt = Math.min(reconnectAttempt + 1, 31)
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null
    start(after)
  }, delay)
}

function start(after?: number) {
  closeSource()
  clearReconnect()
  if (subscribers.size === 0) return

  lastSequence = null
  publishState({ status: 'connecting', cursor: null, gap: null })
  const next = new EventSource(api.events.streamUrl(after), { withCredentials: true })
  source = next

  next.addEventListener('stream.ready', raw => {
    if (source !== next) return
    const ready = parseReady((raw as MessageEvent<string>).data)
    if (!ready) {
      localGap(after)
      return
    }
    lastSequence = ready.cursor
    reconnectAttempt = 0
    publishState({ status: 'ready', cursor: ready.cursor, gap: null })
  })

  next.addEventListener('durable_event', raw => {
    if (source !== next || state.status !== 'ready' || lastSequence === null) return
    const event = parseEnvelope(raw as MessageEvent<string>)
    if (!event || event.sequence !== lastSequence + 1) {
      localGap(lastSequence)
      return
    }
    lastSequence = event.sequence
    state = { status: 'ready', cursor: event.sequence, gap: null }
    publishEvent(event)
  })

  next.addEventListener('stream.gap', raw => {
    if (source !== next) return
    const gap = parseGap((raw as MessageEvent<string>).data)
    if (!gap) {
      localGap(lastSequence ?? after)
      return
    }
    closeSource()
    lastSequence = null
    publishState({ status: 'gap', cursor: null, gap })
    scheduleReconnect(gap.latest_available)
  })

  next.onerror = () => {
    if (source !== next) return
    const reconnectAfter = lastSequence ?? after
    closeSource()
    publishState({ status: 'disconnected', cursor: lastSequence, gap: null })
    scheduleReconnect(reconnectAfter)
  }
}

export function subscribeDurableEvents(subscriber: DurableEventSubscriber): () => void {
  subscribers.add(subscriber)
  subscriber.onState(state)
  if (subscribers.size === 1) start()

  return () => {
    subscribers.delete(subscriber)
    if (subscribers.size === 0) {
      clearReconnect()
      closeSource()
      lastSequence = null
      reconnectAttempt = 0
      state = { status: 'disconnected', cursor: null, gap: null }
    }
  }
}

/** Test-only cleanup for module-level EventSource state. */
export function resetDurableEventStreamForTests() {
  subscribers.clear()
  clearReconnect()
  closeSource()
  lastSequence = null
  reconnectAttempt = 0
  state = { status: 'disconnected', cursor: null, gap: null }
}
