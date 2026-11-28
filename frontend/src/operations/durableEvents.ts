import { parseDurableEventEnvelope } from '@/api/envelopeClient'
import { api } from '@/api/client'
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
const INITIAL_RECONNECT_DELAY_MS = 1_000
const MAX_RECONNECT_DELAY_MS = 30_000

function isSequence(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0
}

function parseReady(data: string, minimumCursor?: number): DurableEventStreamReady | null {
  try {
    const value = asRecord(JSON.parse(data))
    return value
      && hasOnlyKeys(value, ['cursor', 'high_water'])
      && isSequence(value.cursor)
      && isSequence(value.high_water)
      && value.cursor <= value.high_water
      && (minimumCursor === undefined || value.cursor === minimumCursor)
      ? { cursor: value.cursor, high_water: value.high_water }
      : null
  } catch {
    return null
  }
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null
}

function hasOnlyKeys(value: Record<string, unknown>, keys: string[]): boolean {
  const allowed = new Set(keys)
  return Object.keys(value).every(key => allowed.has(key)) && allowed.size === Object.keys(value).length
}

function parseGap(data: string): DurableEventStreamGap | null {
  try {
    const value = asRecord(JSON.parse(data))
    const validReason = value?.reason === 'behind_retention'
      || value?.reason === 'future_cursor'
      || value?.reason === 'discontinuity'
    const validEarliest = value?.earliest_available === null || isSequence(value?.earliest_available)
    const validLatest = isSequence(value?.latest_available)
    const validBounds = value?.earliest_available === null
      || (validLatest && (value.earliest_available as number) <= (value.latest_available as number))
    const validRelationship = value?.reason === 'behind_retention'
      ? value.earliest_available !== null
        && (value.earliest_available as number) > 0
        && (value.requested_after as number) < (value.earliest_available as number) - 1
      : value?.reason === 'future_cursor'
        ? (value.requested_after as number) > (value.latest_available as number)
        : validLatest && (value.requested_after as number) <= (value.latest_available as number)
    return value
      && hasOnlyKeys(value, ['reason', 'requested_after', 'earliest_available', 'latest_available'])
      && validReason
      && isSequence(value.requested_after)
      && validEarliest
      && validLatest
      && validBounds
      && validRelationship
      ? value as unknown as DurableEventStreamGap
      : null
  } catch {
    return null
  }
}

function parseEnvelope(message: MessageEvent<string>): DurableEventEnvelope | null {
  try {
    const id = Number(message.lastEventId)
    return parseDurableEventEnvelope(JSON.parse(message.data), id)
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
    const ready = parseReady((raw as MessageEvent<string>).data, after)
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
    const expectedCursor = lastSequence ?? after
    if (expectedCursor !== undefined && gap.requested_after !== expectedCursor) {
      localGap(expectedCursor)
      return
    }
    const reconnectFloor = lastSequence ?? after
    if (reconnectFloor !== undefined && gap.latest_available < reconnectFloor) {
      localGap(reconnectFloor)
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
