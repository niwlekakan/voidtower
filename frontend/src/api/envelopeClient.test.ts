import { afterEach, describe, expect, it, vi } from 'vitest'
import { api } from './client'
import {
  ApiEnvelopeError,
  parseApiErrorEnvelope,
  parseJobSuccessEnvelope,
  parsePlanSuccessEnvelope,
} from './envelopeClient'

const job = { id: 'job-1', state: 'queued' }

const validJobEnvelope = {
  schema_version: 1,
  resource_id: 'resource-1',
  action: 'container.start',
  job,
}

describe('versioned API envelope adapters', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('parses a v1 job success envelope without changing its payload', () => {
    expect(parseJobSuccessEnvelope(validJobEnvelope)).toEqual(validJobEnvelope)
  })

  it('rejects an incompatible schema version with a bounded client error', () => {
    expect(() => parsePlanSuccessEnvelope({
      schema_version: 2,
      resource_id: 'resource-1',
      action: 'container.start',
      plan: { job_id: 'job-1' },
    })).toThrowError(new ApiEnvelopeError('unsupported_api_schema', 'Unsupported API envelope schema version.', 1))
  })

  it('rejects a success envelope with a non-object payload', () => {
    expect(() => parseJobSuccessEnvelope({
      schema_version: 1, resource_id: 'resource-1', action: 'container.start', job: null,
    })).toThrowError(new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid success envelope.', 1))
  })

  it('parses canonical errors and rejects malformed error bodies', () => {
    expect(parseApiErrorEnvelope({
      error: { code: 'job_not_found', message: 'The requested job does not exist.' },
    })).toEqual({ error: { code: 'job_not_found', message: 'The requested job does not exist.' } })
    expect(() => parseApiErrorEnvelope({ error: { code: 'x', message: 42 } }))
      .toThrowError(new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0))
    expect(() => parseApiErrorEnvelope({ error: { code: 'x', message: 'x'.repeat(1025) } }))
      .toThrowError(new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0))
  })

  it('routes canonical plan and job responses through production parsing', async () => {
    const fetch = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({
        schema_version: 1, resource_id: 'resource-1', action: 'container.start', plan: { job_id: 'job-1' },
      }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(validJobEnvelope), { status: 202 }))
    vi.stubGlobal('fetch', fetch)

    await expect(api.canonicalActions.plan('resource-1', 'container.start')).resolves.toMatchObject({
      schema_version: 1,
      plan: { job_id: 'job-1' },
    })
    await expect(api.canonicalActions.submit('resource-1', 'container.start', {}, 'key-1'))
      .resolves.toMatchObject({ job: { id: 'job-1' } })
  })

  it('turns a canonical HTTP error into the existing bounded client error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
      error: { code: 'job_not_found', message: 'The requested job does not exist.' },
    }), { status: 404 })))

    await expect(api.operationJobs.get('job-1')).rejects.toMatchObject({
      name: 'ApiClientError', code: 'job_not_found', status: 404,
    })
  })

  it('rejects unsafe idempotency header values before making a request', async () => {
    const fetch = vi.fn()
    vi.stubGlobal('fetch', fetch)

    await expect(api.canonicalActions.submit('resource-1', 'container.start', {}, 'bad\nkey'))
      .rejects.toMatchObject({ code: 'invalid_idempotency_key', status: 400 })
    expect(fetch).not.toHaveBeenCalled()
  })
})
