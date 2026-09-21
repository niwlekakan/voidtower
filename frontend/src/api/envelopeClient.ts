import { API_V1_ENVELOPE_CONTRACT } from './generatedApiContract'

const EXPECTED_PLAN_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.plan_success_v1.schema_version
const EXPECTED_JOB_SUCCESS_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.job_success_v1.schema_version
const EXPECTED_JOB_READ_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.job_read_v1.schema_version
const EXPECTED_COLLECTION_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.job_list_v1.schema_version
const EXPECTED_APPROVAL_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.approval_list_v1.schema_version
const EXPECTED_RESOURCE_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.resource_list_v1.schema_version
const EXPECTED_EVENT_HISTORY_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.event_history_v1.schema_version
const EXPECTED_INVENTORY_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.inventory_upload_v1.schema_version
const MAX_ERROR_CODE_LENGTH = 128
const MAX_ERROR_MESSAGE_LENGTH = 1024
const MAX_ERROR_JOB_ID_LENGTH = 128
const MAX_ENVELOPE_FIELD_LENGTH = 256

type RecordValue = Record<string, unknown>

export interface ApiSuccessEnvelope<T> {
  schema_version: number
  resource_id: string
  action: string
  [key: string]: unknown
  payload?: T
}

export interface ApiErrorEnvelope {
  error: {
    code: string
    message: string
    job_id?: string
  }
}

export interface VersionNegotiationErrorEnvelope extends ApiErrorEnvelope {
  error: ApiErrorEnvelope['error'] & { supported_versions: string[] }
}

export class ApiEnvelopeError extends Error {
  constructor(
    public readonly code: 'unsupported_api_schema' | 'invalid_api_envelope' | 'invalid_api_error',
    message: string,
    public readonly supportedSchemaVersion: number,
  ) {
    super(message)
    this.name = 'ApiEnvelopeError'
  }
}

function asRecord(value: unknown): RecordValue | null {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
    ? value as RecordValue
    : null
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0
}

function isBoundedEnvelopeString(value: unknown): value is string {
  return isNonEmptyString(value) && value.trim().length > 0 && value.length <= MAX_ENVELOPE_FIELD_LENGTH
}

function isSequence(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0
}

function isNullableBoundedString(value: unknown): value is string | null {
  return value === null || isBoundedEnvelopeString(value)
}

function isResource(value: unknown): boolean {
  const resource = asRecord(value)
  return Boolean(
    resource
    && isBoundedEnvelopeString(resource.id)
    && isBoundedEnvelopeString(resource.kind)
    && isBoundedEnvelopeString(resource.display_name)
    && isSequence(resource.revision),
  )
}

function isAlias(value: unknown): boolean {
  const alias = asRecord(value)
  return Boolean(
    alias
    && isBoundedEnvelopeString(alias.resource_id)
    && isBoundedEnvelopeString(alias.namespace)
    && isBoundedEnvelopeString(alias.scope_key)
    && isBoundedEnvelopeString(alias.value),
  )
}

function isCapability(value: unknown): boolean {
  const capability = asRecord(value)
  return Boolean(
    capability
    && isBoundedEnvelopeString(capability.resource_id)
    && isBoundedEnvelopeString(capability.action)
    && (capability.availability === 'available' || capability.availability === 'unavailable' || capability.availability === 'unknown')
    && isNullableBoundedString(capability.reason_code)
    && isNullableBoundedString(capability.detail)
    && isSequence(capability.schema_version)
    && isSequence(capability.observed_at),
  )
}

const DURABLE_ACTOR_TYPES = new Set(['human', 'api_token', 'automation', 'plugin', 'node', 'ai', 'system'])

export function parseDurableEventEnvelope(value: unknown, lastEventId: number): import('./types').DurableEventEnvelope {
  const event = asRecord(value)
  const actor = event?.actor
  const actorRecord = actor === null ? null : asRecord(actor)
  const validActor = actor === null || Boolean(
    actorRecord
    && typeof actorRecord.actor_type === 'string'
    && DURABLE_ACTOR_TYPES.has(actorRecord.actor_type)
    && isNullableBoundedString(actorRecord.id)
    && isNullableBoundedString(actorRecord.source),
  )
  if (!event
    || !isSequence(event.sequence)
    || !isSequence(lastEventId)
    || event.sequence !== lastEventId
    || event.schema_version !== API_V1_ENVELOPE_CONTRACT.envelopes.event_v1.schema_version
    || !isBoundedEnvelopeString(event.event_id)
    || !isBoundedEnvelopeString(event.event_type)
    || !isSequence(event.occurred_at)
    || !validActor
    || !isNullableBoundedString(event.resource_id)
    || !isNullableBoundedString(event.job_id)
    || !isNullableBoundedString(event.approval_id)
    || !isBoundedEnvelopeString(event.correlation_id)
    || !isNullableBoundedString(event.causation_id)
    || !Object.prototype.hasOwnProperty.call(event, 'payload')
  ) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid event envelope.', EXPECTED_EVENT_HISTORY_SCHEMA_VERSION)
  }
  return event as unknown as import('./types').DurableEventEnvelope
}

function requireSuccessEnvelope(value: unknown, payloadKey: 'plan' | 'job', expectedSchemaVersion: number): RecordValue {
  const envelope = asRecord(value)
  if (!envelope) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid success envelope.', expectedSchemaVersion)
  }

  if (envelope.schema_version !== expectedSchemaVersion) {
    throw new ApiEnvelopeError('unsupported_api_schema', 'Unsupported API envelope schema version.', expectedSchemaVersion)
  }

  const payload = asRecord(envelope[payloadKey])
  const payloadIdentity = payloadKey === 'plan' ? payload?.job_id : payload?.id
  if (!isBoundedEnvelopeString(envelope.resource_id) || !isBoundedEnvelopeString(envelope.action) || !payload || !isBoundedEnvelopeString(payloadIdentity)) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid success envelope.', expectedSchemaVersion)
  }
  return envelope
}

export function parsePlanSuccessEnvelope<T = unknown>(value: unknown): ApiSuccessEnvelope<T> & { plan: T } {
  return requireSuccessEnvelope(value, 'plan', EXPECTED_PLAN_SCHEMA_VERSION) as ApiSuccessEnvelope<T> & { plan: T }
}

export function parseJobSuccessEnvelope<T = unknown>(value: unknown): ApiSuccessEnvelope<T> & { job: T } {
  return requireSuccessEnvelope(value, 'job', EXPECTED_JOB_SUCCESS_SCHEMA_VERSION) as ApiSuccessEnvelope<T> & { job: T }
}

export function parseJobReadEnvelope<T = unknown>(value: unknown): ApiSuccessEnvelope<T> & { job: T } {
  return requireSuccessEnvelope(value, 'job', EXPECTED_JOB_READ_SCHEMA_VERSION) as ApiSuccessEnvelope<T> & { job: T }
}

function requireCollectionEnvelope(value: unknown, payloadKey: 'jobs' | 'approvals', expectedSchemaVersion: number): RecordValue {
  const envelope = asRecord(value)
  if (!envelope || envelope.schema_version !== expectedSchemaVersion || !Array.isArray(envelope[payloadKey])) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid collection envelope.', expectedSchemaVersion)
  }
  return envelope
}

export function parseJobListEnvelope<T = unknown>(value: unknown): { schema_version: number; jobs: T[] } {
  return requireCollectionEnvelope(value, 'jobs', EXPECTED_COLLECTION_SCHEMA_VERSION) as { schema_version: number; jobs: T[] }
}

export function parseApprovalListEnvelope<T = unknown>(value: unknown): { schema_version: number; approvals: T[] } {
  return requireCollectionEnvelope(value, 'approvals', EXPECTED_APPROVAL_SCHEMA_VERSION) as { schema_version: number; approvals: T[] }
}

export function parseApprovalReadEnvelope<T = unknown>(value: unknown): { schema_version: number; approval: T } {
  const envelope = asRecord(value)
  if (!envelope || envelope.schema_version !== EXPECTED_APPROVAL_SCHEMA_VERSION || !asRecord(envelope.approval)) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid approval envelope.', EXPECTED_APPROVAL_SCHEMA_VERSION)
  }
  return envelope as { schema_version: number; approval: T }
}

export function parseInventoryUploadEnvelope(value: unknown): import('./types').InventoryUploadResponse {
  const envelope = asRecord(value)
  const result = envelope && asRecord(envelope.result)
  const counts = ['linked', 'registered', 'review_required', 'missing']
  if (
    !envelope
    || envelope.schema_version !== EXPECTED_INVENTORY_SCHEMA_VERSION
    || !result
    || !isBoundedEnvelopeString(result.snapshot_id)
    || typeof result.replayed !== 'boolean'
    || counts.some(field => !isSequence(result[field]))
  ) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid inventory envelope.', EXPECTED_INVENTORY_SCHEMA_VERSION)
  }
  return envelope as unknown as import('./types').InventoryUploadResponse
}

export function parseResourceListEnvelope<T = import('./types').DurableResourceRef>(value: unknown): {
  schema_version: number
  resources: T[]
} {
  const envelope = asRecord(value)
  if (!envelope || envelope.schema_version !== EXPECTED_RESOURCE_SCHEMA_VERSION || !Array.isArray(envelope.resources)
    || envelope.resources.some(resource => !isResource(resource))) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid resource envelope.', EXPECTED_RESOURCE_SCHEMA_VERSION)
  }
  return envelope as { schema_version: number; resources: T[] }
}

export function parseResourceReadEnvelope<T = import('./types').DurableResourceRef>(value: unknown): {
  schema_version: number
  resource: T
  aliases: import('./types').DurableResourceAlias[]
  capabilities: import('./types').DurableResourceCapability[]
} {
  const envelope = asRecord(value)
  const resource = envelope?.resource
  const aliases = envelope?.aliases
  const capabilities = envelope?.capabilities
  if (!envelope || envelope.schema_version !== EXPECTED_RESOURCE_SCHEMA_VERSION
    || !isResource(resource)
    || !Array.isArray(aliases) || aliases.some(alias => !isAlias(alias))
    || !Array.isArray(capabilities) || capabilities.some(capability => !isCapability(capability))) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid resource envelope.', EXPECTED_RESOURCE_SCHEMA_VERSION)
  }
  const resourceRecord = resource as RecordValue
  if (aliases.some(alias => (alias as RecordValue).resource_id !== resourceRecord.id)
    || capabilities.some(capability => (capability as RecordValue).resource_id !== resourceRecord.id)) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid resource envelope.', EXPECTED_RESOURCE_SCHEMA_VERSION)
  }
  return envelope as {
    schema_version: number
    resource: T
    aliases: import('./types').DurableResourceAlias[]
    capabilities: import('./types').DurableResourceCapability[]
  }
}

export function parseResourceCapabilitiesEnvelope<T = import('./types').DurableResourceCapability>(value: unknown): {
  schema_version: number
  resource_id: string
  capabilities: T[]
} {
  const envelope = asRecord(value)
  if (!envelope || envelope.schema_version !== EXPECTED_RESOURCE_SCHEMA_VERSION
    || !isBoundedEnvelopeString(envelope.resource_id)
    || !Array.isArray(envelope.capabilities)
    || envelope.capabilities.some(capability => !isCapability(capability)
      || (capability as RecordValue).resource_id !== envelope.resource_id)) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid resource envelope.', EXPECTED_RESOURCE_SCHEMA_VERSION)
  }
  return envelope as { schema_version: number; resource_id: string; capabilities: T[] }
}

export function parseEventHistoryEnvelope<T = import('./types').DurableEventEnvelope>(value: unknown): {
  schema_version: number
  events: T[]
  next_cursor: number
  earliest_available: number | null
  latest_available: number
} {
  const envelope = asRecord(value)
  const events = envelope?.events
  if (!envelope || envelope.schema_version !== EXPECTED_EVENT_HISTORY_SCHEMA_VERSION
    || !Array.isArray(events)
    || !isSequence(envelope.next_cursor)
    || !isSequence(envelope.latest_available)
    || !(envelope.earliest_available === null || isSequence(envelope.earliest_available))) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid event history envelope.', EXPECTED_EVENT_HISTORY_SCHEMA_VERSION)
  }
  const parsed = events.map(event => {
    const eventRecord = asRecord(event)
    return parseDurableEventEnvelope(event, eventRecord?.sequence as number)
  })
  const nextCursor = envelope.next_cursor as number
  const latestAvailable = envelope.latest_available as number
  const earliestAvailable = envelope.earliest_available as number | null
  const ordered = parsed.every((event, index) => index === 0 || event.sequence > parsed[index - 1].sequence)
  const eventsWithinBounds = parsed.every(event =>
    (earliestAvailable === null || event.sequence >= earliestAvailable) && event.sequence <= latestAvailable,
  )
  if ((earliestAvailable !== null && earliestAvailable > latestAvailable)
    || !ordered
    || !eventsWithinBounds
    || (parsed.length > 0 && (parsed[parsed.length - 1].sequence !== nextCursor || nextCursor > latestAvailable))) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid event history envelope.', EXPECTED_EVENT_HISTORY_SCHEMA_VERSION)
  }
  return { ...envelope, events: parsed } as {
    schema_version: number
    events: T[]
    next_cursor: number
    earliest_available: number | null
    latest_available: number
  }
}

export function parseApiErrorEnvelope(value: unknown): ApiErrorEnvelope {
  const envelope = asRecord(value)
  const error = envelope && asRecord(envelope.error)
  if (
    !error ||
    typeof error.code !== 'string' ||
    typeof error.message !== 'string' ||
    !error.code || error.code.length > MAX_ERROR_CODE_LENGTH ||
    !error.message || error.message.length > MAX_ERROR_MESSAGE_LENGTH ||
    ('job_id' in error && (typeof error.job_id !== 'string' || error.job_id.length > MAX_ERROR_JOB_ID_LENGTH))
  ) {
    throw new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0)
  }
  return { error: {
    code: error.code,
    message: error.message,
    ...(typeof error.job_id === 'string' ? { job_id: error.job_id } : {}),
  } }
}

export function parseVersionNegotiationErrorEnvelope(value: unknown): VersionNegotiationErrorEnvelope {
  const envelope = parseApiErrorEnvelope(value)
  const error = asRecord(asRecord(value)?.error)
  const versions: unknown[] | null = error && Array.isArray(error.supported_versions) ? error.supported_versions : null
  if (
    envelope.error.code !== 'unsupported_api_version'
    || !versions
    || versions.length === 0
    || versions.length > 16
    || versions.some(version => !isBoundedEnvelopeString(version))
  ) {
    throw new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0)
  }
  const supportedVersions = versions.filter(isBoundedEnvelopeString)
  return { error: { ...envelope.error, supported_versions: supportedVersions } }
}
