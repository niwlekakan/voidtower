import { afterEach, describe, expect, it, vi } from 'vitest'
import { api } from './client'
import {
  ApiEnvelopeError,
  parseApiErrorEnvelope,
  parseJobSuccessEnvelope,
  parsePlanSuccessEnvelope,
  parseVersionNegotiationErrorEnvelope,
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

  it.each([
    ['a blank resource identity', { ...validJobEnvelope, resource_id: '   ' }],
    ['a blank action', { ...validJobEnvelope, action: '\t' }],
    ['an oversized job identity', { ...validJobEnvelope, job: { id: 'j'.repeat(257) } }],
  ])('rejects %s as an invalid bounded envelope', (_description, body) => {
    expect(() => parseJobSuccessEnvelope(body)).toThrowError(
      new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid success envelope.', 1),
    )
  })

  it('applies bounded identity validation to plan envelopes', () => {
    expect(() => parsePlanSuccessEnvelope({
      schema_version: 1,
      resource_id: 'resource-1',
      action: 'container.start',
      plan: { job_id: 'p'.repeat(257) },
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

  it.each([
    ['missing versions', undefined],
    ['empty versions', []],
    ['too many versions', Array.from({ length: 17 }, () => '1')],
    ['non-string version', [1]],
    ['blank version', ['   ']],
    ['oversized version', ['1'.repeat(257)]],
  ])('rejects %s in a version-negotiation error', (_description, supported_versions) => {
    expect(() => parseVersionNegotiationErrorEnvelope({
      error: {
        code: 'unsupported_api_version',
        message: 'The requested API version is not supported',
        ...(supported_versions === undefined ? {} : { supported_versions }),
      },
    })).toThrowError(new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0))
  })

  it('routes canonical plan and job responses through production parsing', async () => {
    const fetch = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({
        schema_version: 1, resource_id: 'resource-1', action: 'container.start', plan: { job_id: 'job-1' },
      }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(validJobEnvelope), { status: 202 }))
    vi.stubGlobal('fetch', fetch)

    await expect(api.canonicalActions.plan('resource/1', 'container/start')).resolves.toMatchObject({
      schema_version: 1,
      plan: { job_id: 'job-1' },
    })
    await expect(api.canonicalActions.submit('resource/1', 'container/start', { force: true }, 'key-1'))
      .resolves.toMatchObject({ job: { id: 'job-1' } })
    expect(fetch.mock.calls[0][0]).toBe('/api/resources/resource%2F1/actions/container%2Fstart/plan')
    expect(fetch.mock.calls[0][1]).toMatchObject({
      method: 'POST',
      credentials: 'include',
      headers: {
        'Content-Type': 'application/json',
        'x-voidtower-api-version': '1',
      },
      body: JSON.stringify({ input: {} }),
    })
    expect(fetch.mock.calls[1][0]).toBe('/api/resources/resource%2F1/actions/container%2Fstart')
    expect(fetch.mock.calls[1][1]).toMatchObject({
      method: 'POST',
      headers: {
        'Idempotency-Key': 'key-1',
        'x-voidtower-api-version': '1',
      },
      body: JSON.stringify({ input: { force: true } }),
    })
  })

  it('turns a canonical HTTP error into the existing bounded client error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
      error: { code: 'job_not_found', message: 'The requested job does not exist.' },
    }), { status: 404 })))

    await expect(api.operationJobs.get('job-1')).rejects.toMatchObject({
      name: 'ApiClientError', code: 'job_not_found', status: 404,
    })
  })

  it('clears the authenticated identity when the server reports an expired session', async () => {
    const { useAuthStore } = await import('@/store/auth')
    useAuthStore.getState().setUser({
      id: 'user-1', username: 'owner', role: 'owner',
      force_password_change: false, totp_enabled: false, mfa_required: false,
    })
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
      error: { code: 'unauthorized', message: 'Authentication required.' },
    }), { status: 401 })))

    await expect(api.auth.me()).rejects.toMatchObject({ name: 'ApiClientError', status: 401 })
    expect(useAuthStore.getState()).toMatchObject({ user: null, status: 'unauthenticated' })
  })

  it('does not let an older 401 clear a newer authenticated session', async () => {
    const { useAuthStore } = await import('@/store/auth')
    useAuthStore.getState().logout()
    let resolveResponse: ((response: Response) => void) | undefined
    vi.stubGlobal('fetch', vi.fn().mockReturnValue(new Promise<Response>(resolve => { resolveResponse = resolve })))

    const pending = api.auth.me()
    useAuthStore.getState().setUser({
      id: 'user-2', username: 'operator', role: 'operator',
      force_password_change: false, totp_enabled: false, mfa_required: false,
    })
    resolveResponse?.(new Response(JSON.stringify({
      error: { code: 'unauthorized', message: 'Authentication required.' },
    }), { status: 401 }))

    await expect(pending).rejects.toMatchObject({ name: 'ApiClientError', status: 401 })
    expect(useAuthStore.getState()).toMatchObject({ user: { id: 'user-2' }, status: 'authenticated' })
  })

  it('preserves supported versions from the version-negotiation error contract', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
      error: {
        code: 'unsupported_api_version',
        message: 'The requested API version is not supported',
        supported_versions: ['1'],
      },
    }), { status: 406, headers: { 'content-type': 'application/json' } })))

    await expect(api.operationJobs.get('job-1')).rejects.toMatchObject({
      name: 'ApiClientError',
      code: 'unsupported_api_version',
      status: 406,
      supportedVersions: ['1'],
    })
  })

  it('rejects unsafe idempotency header values before making a request', async () => {
    const fetch = vi.fn()
    vi.stubGlobal('fetch', fetch)

    await expect(api.canonicalActions.submit('resource-1', 'container.start', {}, 'bad\nkey'))
      .rejects.toMatchObject({ code: 'invalid_idempotency_key', status: 400 })
    expect(fetch).not.toHaveBeenCalled()
  })

  it.each([
    ['an empty resource identity', '', 'container.start'],
    ['a dot resource identity', '.', 'container.start'],
    ['a parent-dot resource identity', '..', 'container.start'],
    ['a blank action', 'resource-1', '  '],
    ['an oversized action', 'resource-1', 'a'.repeat(257)],
  ])('rejects %s before making a canonical request', async (_description, resourceId, action) => {
    const fetch = vi.fn()
    vi.stubGlobal('fetch', fetch)

    await expect(api.canonicalActions.plan(resourceId, action)).rejects.toMatchObject({
      name: 'ApiClientError', code: 'invalid_action_target', status: 400,
    })
    expect(fetch).not.toHaveBeenCalled()
  })
})
