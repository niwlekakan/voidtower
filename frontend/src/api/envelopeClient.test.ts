import { afterEach, describe, expect, it, vi } from 'vitest'
import { api } from './client'
import {
  ApiEnvelopeError,
  parseApprovalListEnvelope,
  parseApprovalReadEnvelope,
  parseApiErrorEnvelope,
  parseEventHistoryEnvelope,
  parseInventoryUploadEnvelope,
  parseJobListEnvelope,
  parseJobSuccessEnvelope,
  parsePlanSuccessEnvelope,
  parseResourceCapabilitiesEnvelope,
  parseResourceListEnvelope,
  parseResourceReadEnvelope,
  parseVersionNegotiationErrorEnvelope,
} from './envelopeClient'

const plan = {
  action: 'container.start',
  resource: { id: 'resource-1', kind: 'container', display_name: 'Example container', revision: 1 },
  input_schema_id: 'container.start.input.v1',
  result_schema_id: 'container.start.result.v1',
  operation: {
    schema_version: 1,
    title: 'Start the web container',
    risk: 'mutate',
    changes: [],
    preview: null,
    external_fingerprint: 'container-stopped',
    steps: [{ kind: 'execute', name: 'Start container', retry_class: 'never', recovery_class: 'reconcile' }],
  },
  policy: { outcome: 'require_approval', reason: 'Action registry requires approval' },
}

const job = {
  id: 'job-1',
  resource: { id: 'resource-1', kind: 'container', display_name: 'Example container', revision: 1 },
  action: 'container.start',
  actor: { actor_type: 'system', id: null, source: 'fixture' },
  ingress: 'api',
  state: 'queued',
  progress_current: 0,
  progress_total: 1,
  progress_message: null,
  plan: plan.operation,
  approval_id: null,
  result: null,
  error: null,
  submitted_at: 1,
  started_at: null,
  finished_at: null,
  updated_at: 1,
}

const validJobEnvelope = {
  schema_version: 1,
  resource_id: 'resource-1',
  action: 'container.start',
  job,
}

const validPlanEnvelope = {
  schema_version: 1,
  resource_id: 'resource-1',
  action: 'container.start',
  plan,
}

const resource = { id: 'resource-1', kind: 'container', display_name: 'Example container', revision: 1 }
const event = {
  sequence: 1,
  event_id: 'event-1',
  schema_version: 1,
  event_type: 'job.running.v1',
  occurred_at: 100,
  actor: null,
  resource_id: 'resource-1',
  job_id: 'job-1',
  approval_id: null,
  correlation_id: 'correlation-1',
  causation_id: null,
  payload: { state: 'running' },
}

describe('versioned API envelope adapters', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('parses a v1 job success envelope without changing its payload', () => {
    expect(parseJobSuccessEnvelope(validJobEnvelope)).toEqual(validJobEnvelope)
  })

  it.each([
    ['a missing job resource identity', { ...job, resource: undefined }],
    ['a mismatched job resource identity', { ...job, resource: { ...job.resource, id: 'resource-2' } }],
    ['a mismatched job action identity', { ...job, action: 'container.stop' }],
  ])('rejects %s in a job success envelope', (_description, invalidJob) => {
    expect(() => parseJobSuccessEnvelope({ ...validJobEnvelope, job: invalidJob })).toThrow(ApiEnvelopeError)
  })

  it('parses the complete source-owned PlanView payload and binds its identities', () => {
    expect(parsePlanSuccessEnvelope(validPlanEnvelope)).toEqual(validPlanEnvelope)
  })

  it('parses versioned job and approval collection envelopes', () => {
    expect(parseJobListEnvelope({ schema_version: 1, jobs: [job] })).toEqual({ schema_version: 1, jobs: [job] })
    expect(parseApprovalListEnvelope({ schema_version: 1, approvals: [] })).toEqual({ schema_version: 1, approvals: [] })
    expect(parseApprovalReadEnvelope({
      schema_version: 1,
      approval: {
        id: 'approval-1', job_id: 'job-1', requirement: 'operator', reason: 'policy', status: 'pending',
        expires_at: 100, decided_by: null, decision_comment: null, requested_at: 1, decided_at: null, updated_at: 1,
      },
    })).toEqual({
      schema_version: 1,
      approval: {
        id: 'approval-1', job_id: 'job-1', requirement: 'operator', reason: 'policy', status: 'pending',
        expires_at: 100, decided_by: null, decision_comment: null, requested_at: 1, decided_at: null, updated_at: 1,
      },
    })
  })

  it('rejects oversized result bytes, oversized job collections, and unknown job fields', () => {
    const oversizedResult = Array.from({ length: 64 }, () => 'x'.repeat(4160))
    expect(() => parseJobSuccessEnvelope({ ...validJobEnvelope, job: { ...job, result: oversizedResult } }))
      .toThrow(ApiEnvelopeError)
    expect(() => parseJobListEnvelope({ schema_version: 1, jobs: Array.from({ length: 257 }, () => job) }))
      .toThrow(ApiEnvelopeError)
    expect(() => parseJobSuccessEnvelope({ ...validJobEnvelope, job: { ...job, unexpected: true } }))
      .toThrow(ApiEnvelopeError)
    expect(() => parseJobSuccessEnvelope({
      ...validJobEnvelope,
      job: { ...job, error: { code: 'failed', message: 'bounded', retryable: false, job_id: null, unexpected: true } },
    })).toThrow(ApiEnvelopeError)
  })

  it('rejects contradictory job progress and terminal payload fields', () => {
    expect(() => parseJobSuccessEnvelope({
      ...validJobEnvelope,
      job: { ...job, progress_current: 4, progress_total: 3 },
    })).toThrow(ApiEnvelopeError)
    expect(() => parseJobSuccessEnvelope({
      ...validJobEnvelope,
      job: { ...job, result: { ok: true }, error: { code: 'failed', message: 'also failed', retryable: false, job_id: null } },
    })).toThrow(ApiEnvelopeError)
  })

  it('parses the versioned inventory upload result envelope', () => {
    expect(parseInventoryUploadEnvelope({
      schema_version: 1,
      result: {
        snapshot_id: 'snapshot-1', replayed: false, linked: 1, registered: 0,
        review_required: 0, missing: 0,
      },
    })).toMatchObject({ schema_version: 1, result: { snapshot_id: 'snapshot-1', linked: 1 } })
  })

  it.each([
    ['an unsupported schema', { schema_version: 2, result: {} }],
    ['a malformed snapshot identity', { schema_version: 1, result: { snapshot_id: '', replayed: false, linked: 0, registered: 0, review_required: 0, missing: 0 } }],
    ['a negative result count', { schema_version: 1, result: { snapshot_id: 'snapshot-1', replayed: false, linked: -1, registered: 0, review_required: 0, missing: 0 } }],
  ])('rejects %s in the inventory result envelope', (_description, body) => {
    expect(() => parseInventoryUploadEnvelope(body)).toThrowError(ApiEnvelopeError)
  })

  it('parses source-owned resource and event-history envelopes', () => {
    expect(parseResourceListEnvelope({ schema_version: 1, resources: [resource] }))
      .toEqual({ schema_version: 1, resources: [resource] })
    expect(parseResourceReadEnvelope({
      schema_version: 1,
      resource,
      aliases: [{ resource_id: 'resource-1', namespace: 'provider', scope_key: 'default', value: 'container-1' }],
      capabilities: [],
    })).toMatchObject({ schema_version: 1, resource })
    expect(parseResourceCapabilitiesEnvelope({ schema_version: 1, resource_id: 'resource-1', capabilities: [] }))
      .toEqual({ schema_version: 1, resource_id: 'resource-1', capabilities: [] })
    expect(parseEventHistoryEnvelope({
      schema_version: 1,
      events: [event],
      next_cursor: 1,
      earliest_available: 1,
      latest_available: 1,
    })).toMatchObject({ schema_version: 1, events: [event], next_cursor: 1 })
  })

  it.each([
    ['an incompatible resource version', { schema_version: 2, resources: [] }],
    ['a malformed resource identity', { schema_version: 1, resources: [{ ...resource, revision: -1 }] }],
    ['an unknown event actor type', { schema_version: 1, events: [{ ...event, actor: { actor_type: 'future', id: null, source: null } }], next_cursor: 1, earliest_available: 1, latest_available: 1 }],
    ['a non-monotonic history cursor', { schema_version: 1, events: [event], next_cursor: 0, earliest_available: 1, latest_available: 1 }],
    ['an event outside retained bounds', { schema_version: 1, events: [event], next_cursor: 1, earliest_available: 2, latest_available: 3 }],
    ['inverted retained bounds', { schema_version: 1, events: [], next_cursor: 0, earliest_available: 2, latest_available: 1 }],
  ])('rejects %s at the source-owned read contract', (_description, body) => {
    expect(() => {
      if ('resources' in body) return parseResourceListEnvelope(body)
      return parseEventHistoryEnvelope(body)
    }).toThrowError(ApiEnvelopeError)
  })

  it('rejects unversioned collection envelopes', () => {
    expect(() => parseJobListEnvelope({ jobs: [] })).toThrowError(
      new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid collection envelope.', 1),
    )
  })

  it('rejects an incompatible schema version with a bounded client error', () => {
    expect(() => parsePlanSuccessEnvelope({
      schema_version: 2,
      resource_id: 'resource-1',
      action: 'container.start',
      plan,
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
      plan: { ...plan, operation: { ...plan.operation, title: 'p'.repeat(257) } },
    })).toThrowError(new ApiEnvelopeError('invalid_api_envelope', 'The API returned an invalid success envelope.', 1))
  })

  it.each([
    ['a scalar input schema identity', { ...plan, input_schema_id: 'container.start.input.v0' }],
    ['a non-ASCII input schema identity', { ...plan, input_schema_id: '😀.input.v1' }],
    ['an oversized input schema identity', { ...plan, input_schema_id: `${'a'.repeat(250)}.input.v1` }],
    ['an action schema identity from another action', { ...plan, input_schema_id: 'container.stop.input.v1', result_schema_id: 'container.stop.result.v1' }],
    ['an empty operation step list', { ...plan, operation: { ...plan.operation, steps: [] } }],
    ['a mismatched resource identity', { ...plan, resource: { ...plan.resource, id: 'resource-2' } }],
  ])('rejects %s in a PlanView payload', (_description, invalidPlan) => {
    expect(() => parsePlanSuccessEnvelope({ ...validPlanEnvelope, plan: invalidPlan })).toThrowError(ApiEnvelopeError)
  })

  it('accepts the documented long preview and policy reason bounds', () => {
    const longPreview = 'x'.repeat(16 * 1024)
    const longReason = 'x'.repeat(1024)
    expect(parsePlanSuccessEnvelope({
      ...validPlanEnvelope,
      plan: {
        ...plan,
        operation: { ...plan.operation, preview: longPreview },
        policy: { ...plan.policy, reason: longReason },
      },
    })).toEqual({
      ...validPlanEnvelope,
      plan: {
        ...plan,
        operation: { ...plan.operation, preview: longPreview },
        policy: { ...plan.policy, reason: longReason },
      },
    })
  })

  it('parses canonical errors and rejects malformed error bodies', () => {
    expect(parseApiErrorEnvelope({
      error: { code: 'job_not_found', message: 'The requested job does not exist.' },
    })).toEqual({ error: { code: 'job_not_found', message: 'The requested job does not exist.' } })
    expect(() => parseApiErrorEnvelope({ error: { code: 'x', message: 42 } }))
      .toThrowError(new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0))
    expect(() => parseApiErrorEnvelope({ error: { code: 'x', message: 'x'.repeat(1025) } }))
      .toThrowError(new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0))
    expect(() => parseApiErrorEnvelope({ error: { code: 'x', message: 'x', unexpected: true } }))
      .toThrowError(new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0))
    expect(() => parseApiErrorEnvelope({ error: { code: 'x', message: 'x' }, unexpected: true }))
      .toThrowError(new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0))
    expect(() => parseApiErrorEnvelope({ error: { code: 'job_not_found', message: 'x', supported_versions: ['1'] } }))
      .toThrowError(new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0))
    expect(parseApiErrorEnvelope({ error: { code: 'x', message: '😀'.repeat(1024) } }).error.message)
      .toHaveLength(2048)
    expect(() => parseApiErrorEnvelope({ error: { code: 'x', message: '😀'.repeat(1025) } }))
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
        job_id: 'job-1',
        supported_versions: ['1'],
      },
    })).toThrowError(new ApiEnvelopeError('invalid_api_error', 'The API returned an invalid error envelope.', 0))
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
        ...validPlanEnvelope,
      }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(validJobEnvelope), { status: 202 }))
    vi.stubGlobal('fetch', fetch)

    await expect(api.canonicalActions.plan('resource/1', 'container/start')).resolves.toMatchObject({
      schema_version: 1,
      plan: { action: 'container.start', input_schema_id: 'container.start.input.v1' },
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
