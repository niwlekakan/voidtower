import { API_V1_ENVELOPE_CONTRACT } from './generatedApiContract'

const EXPECTED_PLAN_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.plan_success_v1.schema_version
const EXPECTED_JOB_SUCCESS_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.job_success_v1.schema_version
const EXPECTED_JOB_READ_SCHEMA_VERSION = API_V1_ENVELOPE_CONTRACT.envelopes.job_read_v1.schema_version
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
