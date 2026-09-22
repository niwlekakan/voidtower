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

function hasOnlyKeys(record: RecordValue, allowed: readonly string[]): boolean {
  const allowedKeys = new Set(allowed)
  return Object.keys(record).every(key => allowedKeys.has(key))
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0
}

function isBoundedEnvelopeString(value: unknown): value is string {
  return isNonEmptyString(value) && value.trim().length > 0 && Array.from(value).length <= MAX_ENVELOPE_FIELD_LENGTH
}

function isSequence(value: unknown): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= 0
}

function isNullableBoundedString(value: unknown): value is string | null {
  return value === null || isBoundedEnvelopeString(value)
}

function isNullablePlanText(value: unknown, maxLength: number): value is string | null {
  return value === null
    || (typeof value === 'string' && Array.from(value).length <= maxLength)
}

function isPlanText(value: unknown, maxLength: number): value is string {
  return typeof value === 'string'
    && Array.from(value).length > 0
    && Array.from(value).length <= maxLength
}

function isPlanValue(value: unknown, maxLength: number): value is string {
  return typeof value === 'string' && Array.from(value).length <= maxLength
}

function isResource(value: unknown): boolean {
  const resource = asRecord(value)
  return Boolean(
    resource
    && hasOnlyKeys(resource, ['id', 'kind', 'display_name', 'revision'])
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
    && hasOnlyKeys(alias, ['resource_id', 'namespace', 'scope_key', 'value'])
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
    && hasOnlyKeys(capability, ['resource_id', 'action', 'availability', 'reason_code', 'detail', 'schema_version', 'observed_at'])
    && isBoundedEnvelopeString(capability.resource_id)
    && isBoundedEnvelopeString(capability.action)
    && (capability.availability === 'available' || capability.availability === 'unavailable' || capability.availability === 'unknown')
    && isNullableBoundedString(capability.reason_code)
    && isNullableBoundedString(capability.detail)
    && isSequence(capability.schema_version)
    && isSequence(capability.observed_at),
  )
}

function isPlanChange(value: unknown): boolean {
  const change = asRecord(value)
  return Boolean(
    change
    && hasOnlyKeys(change, ['label', 'value'])
    && typeof change.label === 'string'
    && change.label.length > 0
    && Array.from(change.label).length <= 128
    && isPlanValue(change.value, 2 * 1024),
  )
}

function isPlannedStep(value: unknown): boolean {
  const step = asRecord(value)
  return Boolean(
    step
    && hasOnlyKeys(step, ['kind', 'name', 'retry_class', 'recovery_class'])
    && isPlanText(step.kind, 64)
    && isPlanText(step.name, 256)
    && isPlanText(step.retry_class, 64)
    && isPlanText(step.recovery_class, 64),
  )
}

function isOperationPlan(value: unknown): boolean {
  const operation = asRecord(value)
  return Boolean(
    operation
    && hasOnlyKeys(operation, ['schema_version', 'title', 'risk', 'changes', 'preview', 'external_fingerprint', 'steps'])
    && operation.schema_version === 1
    && isPlanText(operation.title, 256)
    && isPlanText(operation.risk, 64)
    && Array.isArray(operation.changes)
    && operation.changes.length <= 64
    && operation.changes.every(isPlanChange)
    && isNullablePlanText(operation.preview, 16 * 1024)
    && isPlanText(operation.external_fingerprint, 256)
    && Array.isArray(operation.steps)
    && operation.steps.length > 0
    && operation.steps.length <= 64
    && operation.steps.every(isPlannedStep)
  )
}

function isPolicyPreview(value: unknown): boolean {
  const policy = asRecord(value)
  return Boolean(
    policy
    && hasOnlyKeys(policy, ['outcome', 'reason'])
    && (policy.outcome === 'allow' || policy.outcome === 'require_approval' || policy.outcome === 'deny')
    && isNullablePlanText(policy.reason, 1024),
  )
}

function isActionSchemaId(value: unknown, action: unknown, suffix: 'input' | 'result'): boolean {
  return typeof value === 'string'
    && typeof action === 'string'
    && value.length <= 256
    && /^[A-Za-z0-9][A-Za-z0-9._:-]{0,255}\.(input|result)\.v1$/.test(value)
    && value === `${action}.${suffix}.v1`
}

function isPlanView(value: unknown): value is import('./types').DurablePlanView {
  const plan = asRecord(value)
  return Boolean(
    plan
    && hasOnlyKeys(plan, ['action', 'resource', 'input_schema_id', 'result_schema_id', 'operation', 'policy'])
    && isBoundedEnvelopeString(plan.action)
    && isResource(plan.resource)
    && isActionSchemaId(plan.input_schema_id, plan.action, 'input')
    && isActionSchemaId(plan.result_schema_id, plan.action, 'result')
    && isOperationPlan(plan.operation)
    && isPolicyPreview(plan.policy)
  )
}

const DURABLE_ACTOR_TYPES = new Set(['human', 'api_token', 'automation', 'plugin', 'node', 'ai', 'system'])

function isBoundedResultValue(value: unknown, depth = 0): boolean {
  if (depth > 8) return false
  let serialized: string | undefined
  try {
    serialized = JSON.stringify(value)
  } catch {
    return false
  }
  if (serialized === undefined || new TextEncoder().encode(serialized).length > 64 * 1024) return false
  if (value === null || typeof value === 'boolean') return true
  if (typeof value === 'number') return Number.isFinite(value)
  if (typeof value === 'string') return Array.from(value).length <= 4160
  if (Array.isArray(value)) return value.length <= 64 && value.every(item => isBoundedResultValue(item, depth + 1))
  const record = asRecord(value)
  return Boolean(
    record
    && Object.keys(record).length <= 64
    && Object.keys(record).every(key => Array.from(key).length <= 128)
    && Object.values(record).every(item => isBoundedResultValue(item, depth + 1)),
  )
}

function isOperationError(value: unknown): boolean {
  const error = asRecord(value)
  return Boolean(
    error
    && hasOnlyKeys(error, ['code', 'message', 'retryable', 'job_id'])
    && isPlanText(error.code, 128)
    && isPlanText(error.message, 1024)
    && typeof error.retryable === 'boolean'
    && (error.job_id === null || isPlanText(error.job_id, 128)),
  )
}

function isJobView(value: unknown, expectedResourceId?: unknown, expectedAction?: unknown): boolean {
  const job = asRecord(value)
  const resource = job && asRecord(job.resource)
  const actor = job && asRecord(job.actor)
  return Boolean(
    job
    && hasOnlyKeys(job, ['id', 'action', 'resource', 'actor', 'ingress', 'state', 'progress_current', 'progress_total', 'progress_message', 'plan', 'approval_id', 'result', 'error', 'submitted_at', 'started_at', 'finished_at', 'updated_at'])
    && isPlanText(job.id, 128)
    && isPlanText(job.action, 256)
    && isResource(resource)
    && (expectedResourceId === undefined || resource?.id === expectedResourceId)
    && (expectedAction === undefined || job.action === expectedAction)
    && actor
    && hasOnlyKeys(actor, ['actor_type', 'id', 'source'])
    && typeof actor.actor_type === 'string'
    && DURABLE_ACTOR_TYPES.has(actor.actor_type)
    && isNullableBoundedString(actor.id)
    && isNullableBoundedString(actor.source)
    && isPlanText(job.ingress, 64)
    && typeof job.state === 'string'
    && ['awaiting_approval', 'queued', 'running', 'succeeded', 'failed', 'cancelled', 'needs_attention', 'rejected', 'expired'].includes(job.state)
    && isSequence(job.progress_current)
    && isSequence(job.progress_total)
    && job.progress_current <= job.progress_total
    && isNullablePlanText(job.progress_message, 1024)
    && isOperationPlan(job.plan)
    && job.progress_total === (job.plan as { steps: unknown[] }).steps.length
    && (job.approval_id === null || isPlanText(job.approval_id, 128))
    && (job.result === null || isBoundedResultValue(job.result))
    && (job.error === null || isOperationError(job.error))
    && !(job.result !== null && job.error !== null)
    && (job.state !== 'succeeded' || (job.result !== null && job.error === null))
    && (!['failed', 'needs_attention', 'rejected', 'expired'].includes(job.state) || job.error !== null)
    && Number.isSafeInteger(job.submitted_at)
    && (job.started_at === null || Number.isSafeInteger(job.started_at))
    && (job.finished_at === null || Number.isSafeInteger(job.finished_at))
    && Number.isSafeInteger(job.updated_at),
  )
}

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
    || !hasOnlyKeys(event, ['sequence', 'event_id', 'event_type', 'schema_version', 'occurred_at', 'actor', 'resource_id', 'job_id', 'approval_id', 'correlation_id', 'causation_id', 'payload'])
    || (actorRecord !== null && !hasOnlyKeys(actorRecord, ['actor_type', 'id', 'source']))
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
  const validPayload = payloadKey === 'plan'
    ? isPlanView(payload)
    : Boolean(
      payload
      && isJobView(payload, envelope.resource_id, envelope.action),
    )
  if (!hasOnlyKeys(envelope, ['schema_version', 'resource_id', 'action', payloadKey])
    || !isBoundedEnvelopeString(envelope.resource_id) || !isBoundedEnvelopeString(envelope.action) || !payload || !validPayload) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid success envelope.', expectedSchemaVersion)
  }
  if (payloadKey === 'plan'
    && ((payload.resource as RecordValue).id !== envelope.resource_id || payload.action !== envelope.action)) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid success envelope.', expectedSchemaVersion)
  }
  return envelope
}

export function parsePlanSuccessEnvelope<T = import('./types').DurablePlanView>(value: unknown): ApiSuccessEnvelope<T> & { plan: T } {
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
  if (!envelope || envelope.schema_version !== expectedSchemaVersion
    || !hasOnlyKeys(envelope, ['schema_version', payloadKey])
    || !Array.isArray(envelope[payloadKey])
    || (payloadKey === 'jobs' && (envelope.jobs as unknown[]).length > 256)
    || (payloadKey === 'jobs' && (envelope.jobs as unknown[]).some(job => !isJobView(job)))
    || (payloadKey === 'approvals' && (envelope.approvals as unknown[]).some(approval => !isApproval(approval)))) {
    throw new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid collection envelope.', expectedSchemaVersion)
  }
  return envelope
}

function isApproval(value: unknown): boolean {
  const approval = asRecord(value)
  return Boolean(
    approval
    && hasOnlyKeys(approval, ['id', 'job_id', 'requirement', 'reason', 'status', 'expires_at', 'decided_by', 'decision_comment', 'requested_at', 'decided_at', 'updated_at'])
    && isPlanText(approval.id, 128)
    && isPlanText(approval.job_id, 128)
    && isPlanText(approval.requirement, 64)
    && isPlanText(approval.reason, 1024)
    && ['pending', 'approved', 'rejected', 'expired', 'stale'].includes(approval.status as string)
    && Number.isSafeInteger(approval.expires_at)
    && isNullableBoundedString(approval.decided_by)
    && isNullablePlanText(approval.decision_comment, 1024)
    && Number.isSafeInteger(approval.requested_at)
    && (approval.decided_at === null || Number.isSafeInteger(approval.decided_at))
    && Number.isSafeInteger(approval.updated_at)
  )
}

export function parseJobListEnvelope<T = unknown>(value: unknown): { schema_version: number; jobs: T[] } {
  return requireCollectionEnvelope(value, 'jobs', EXPECTED_COLLECTION_SCHEMA_VERSION) as { schema_version: number; jobs: T[] }
}

export function parseApprovalListEnvelope<T = unknown>(value: unknown): { schema_version: number; approvals: T[] } {
  return requireCollectionEnvelope(value, 'approvals', EXPECTED_APPROVAL_SCHEMA_VERSION) as { schema_version: number; approvals: T[] }
}

export function parseApprovalReadEnvelope<T = unknown>(value: unknown): { schema_version: number; approval: T } {
  const envelope = asRecord(value)
  if (!envelope || envelope.schema_version !== EXPECTED_APPROVAL_SCHEMA_VERSION
    || !hasOnlyKeys(envelope, ['schema_version', 'approval']) || !isApproval(envelope.approval)) {
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
    || !hasOnlyKeys(envelope, ['schema_version', 'result'])
    || !result
    || !hasOnlyKeys(result, ['snapshot_id', 'replayed', 'linked', 'registered', 'review_required', 'missing'])
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
  if (!envelope || envelope.schema_version !== EXPECTED_RESOURCE_SCHEMA_VERSION
    || !hasOnlyKeys(envelope, ['schema_version', 'resources']) || !Array.isArray(envelope.resources)
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
    || !hasOnlyKeys(envelope, ['schema_version', 'resource', 'aliases', 'capabilities'])
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
    || !hasOnlyKeys(envelope, ['schema_version', 'resource_id', 'capabilities'])
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
    || !hasOnlyKeys(envelope, ['schema_version', 'events', 'next_cursor', 'earliest_available', 'latest_available'])
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
  const hasSupportedVersions = Boolean(error && 'supported_versions' in error)
  const isVersionEnvelope = error?.code === 'unsupported_api_version' && hasSupportedVersions
  if (
    !error ||
    !hasOnlyKeys(error, isVersionEnvelope ? ['code', 'message', 'supported_versions'] : ['code', 'message', 'job_id']) ||
    !hasOnlyKeys(envelope ?? {}, ['error']) ||
    typeof error.code !== 'string' ||
    typeof error.message !== 'string' ||
    !error.code || Array.from(error.code).length > MAX_ERROR_CODE_LENGTH ||
    !error.message || Array.from(error.message).length > MAX_ERROR_MESSAGE_LENGTH ||
    ('job_id' in error && (typeof error.job_id !== 'string' || Array.from(error.job_id).length > MAX_ERROR_JOB_ID_LENGTH))
    || (isVersionEnvelope && (!Array.isArray(error.supported_versions)
      || error.supported_versions.length === 0
      || error.supported_versions.length > 16
      || error.supported_versions.some(version => !isBoundedEnvelopeString(version))))
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
